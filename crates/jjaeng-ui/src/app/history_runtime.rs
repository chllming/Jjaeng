use std::cell::RefCell;
use std::rc::Rc;

use gtk4::prelude::*;
use gtk4::{
    Align, Application, ApplicationWindow, Box as GtkBox, Button, FlowBox, Frame, GestureClick,
    Label, Orientation, Picture, PolicyType, ScrolledWindow, SelectionMode,
};
use jjaeng_core::clipboard::{ClipboardBackend, WlCopyBackend};
use jjaeng_core::history::{HistoryEntry, HistoryService};
use jjaeng_core::notification;
use jjaeng_core::state::{AppEvent, AppState, StateMachine};
use jjaeng_core::storage::StorageService;

use super::hypr::{focused_monitor_geometry, request_window_floating_with_geometry};
use super::runtime_support::RuntimeSession;
use crate::ui::StyleTokens;

const HISTORY_WINDOW_TITLE: &str = "Jjaeng History";
const HISTORY_WINDOW_WIDTH: i32 = 860;
const HISTORY_WINDOW_HEIGHT: i32 = 620;

#[derive(Clone)]
pub(super) struct HistoryRenderContext {
    pub(super) app: Application,
    pub(super) style_tokens: StyleTokens,
    pub(super) status_log: Rc<RefCell<String>>,
    pub(super) runtime_session: Rc<RefCell<RuntimeSession>>,
    pub(super) shared_machine: Rc<RefCell<StateMachine>>,
    pub(super) storage_service: Rc<Option<StorageService>>,
    pub(super) history_service: Rc<Option<HistoryService>>,
    pub(super) editor_has_unsaved_changes: Rc<RefCell<bool>>,
    pub(super) history_window: Rc<RefCell<Option<HistoryWindowRuntime>>>,
}

#[derive(Clone)]
pub(super) struct HistoryWindowRuntime {
    pub(super) window: ApplicationWindow,
    count_label: Label,
    empty_state_label: Label,
    flow_box: FlowBox,
}

pub(super) fn present_history_window(context: &HistoryRenderContext, render: &Rc<dyn Fn()>) {
    let existing_runtime = { context.history_window.borrow().as_ref().cloned() };
    let runtime = if let Some(runtime) = existing_runtime {
        runtime
    } else {
        close_duplicate_history_windows(&context.app, None);
        let runtime = build_history_window(context);
        context.history_window.borrow_mut().replace(runtime.clone());
        runtime
    };

    close_duplicate_history_windows(&context.app, Some(&runtime.window));
    if let Some((x, y, width, height)) = history_window_geometry(context.style_tokens) {
        runtime.window.set_default_size(width, height);
        runtime.window.set_size_request(width, height);
        request_window_floating_with_geometry(
            "history",
            HISTORY_WINDOW_TITLE,
            true,
            Some((x, y, width, height)),
        );
    }
    runtime.window.present();
    refresh_history_window_if_open(context, render);
}

pub(super) fn toggle_history_window(context: &HistoryRenderContext, render: &Rc<dyn Fn()>) {
    if close_history_window_if_open(context) {
        return;
    }

    present_history_window(context, render);
}

pub(super) fn refresh_history_window_if_open(
    context: &HistoryRenderContext,
    render: &Rc<dyn Fn()>,
) {
    let Some(runtime) = context.history_window.borrow().as_ref().cloned() else {
        return;
    };

    let Some(history_service) = context.history_service.as_ref().as_ref() else {
        clear_flow_box(&runtime.flow_box);
        runtime.count_label.set_text("History unavailable");
        runtime
            .empty_state_label
            .set_text("History service unavailable.");
        runtime.empty_state_label.set_visible(true);
        return;
    };

    let entries = match history_service.list_entries() {
        Ok(entries) => entries,
        Err(err) => {
            clear_flow_box(&runtime.flow_box);
            runtime.count_label.set_text("History failed");
            runtime
                .empty_state_label
                .set_text(&format!("Failed to load history: {err}"));
            runtime.empty_state_label.set_visible(true);
            return;
        }
    };

    runtime.count_label.set_text(&format!(
        "{} item{}",
        entries.len(),
        if entries.len() == 1 { "" } else { "s" }
    ));
    runtime.empty_state_label.set_visible(entries.is_empty());
    if entries.is_empty() {
        runtime
            .empty_state_label
            .set_text("Take a screenshot and it will appear here.");
    }

    clear_flow_box(&runtime.flow_box);
    for entry in entries {
        let tile = build_history_tile(context, render, &entry);
        runtime.flow_box.insert(&tile, -1);
    }
}

fn build_history_window(context: &HistoryRenderContext) -> HistoryWindowRuntime {
    let window = ApplicationWindow::new(&context.app);
    window.set_title(Some(HISTORY_WINDOW_TITLE));
    window.add_css_class(jjaeng_core::identity::APP_CSS_ROOT);
    window.add_css_class("history-window");
    window.set_default_size(HISTORY_WINDOW_WIDTH, HISTORY_WINDOW_HEIGHT);
    window.set_size_request(HISTORY_WINDOW_WIDTH, HISTORY_WINDOW_HEIGHT);
    window.set_resizable(false);
    window.set_decorated(false);

    let root = GtkBox::new(Orientation::Vertical, context.style_tokens.spacing_12);
    root.set_margin_top(context.style_tokens.spacing_16);
    root.set_margin_bottom(context.style_tokens.spacing_16);
    root.set_margin_start(context.style_tokens.spacing_16);
    root.set_margin_end(context.style_tokens.spacing_16);
    root.add_css_class("history-root");

    let header_row = GtkBox::new(Orientation::Horizontal, context.style_tokens.spacing_8);
    header_row.set_halign(Align::Fill);

    let title_stack = GtkBox::new(Orientation::Vertical, context.style_tokens.spacing_4);
    title_stack.set_hexpand(true);

    let title_label = Label::new(Some(HISTORY_WINDOW_TITLE));
    title_label.add_css_class("history-title");
    title_label.set_halign(Align::Start);
    title_label.set_xalign(0.0);

    let subtitle_label = Label::new(Some(
        "Recent captures with quick copy, save, and edit actions.",
    ));
    subtitle_label.add_css_class("history-subtitle");
    subtitle_label.set_halign(Align::Start);
    subtitle_label.set_xalign(0.0);

    let count_label = Label::new(Some("0 items"));
    count_label.add_css_class("history-count");
    count_label.set_halign(Align::End);
    count_label.set_valign(Align::Start);

    title_stack.append(&title_label);
    title_stack.append(&subtitle_label);
    header_row.append(&title_stack);
    header_row.append(&count_label);

    let flow_box = FlowBox::new();
    flow_box.set_selection_mode(SelectionMode::None);
    flow_box.set_column_spacing(context.style_tokens.spacing_12 as u32);
    flow_box.set_row_spacing(context.style_tokens.spacing_12 as u32);
    flow_box.set_max_children_per_line(3);
    flow_box.set_activate_on_single_click(false);
    flow_box.add_css_class("history-grid");

    let scroller = ScrolledWindow::new();
    scroller.set_policy(PolicyType::Never, PolicyType::Automatic);
    scroller.set_vexpand(true);
    scroller.set_hexpand(true);
    scroller.set_child(Some(&flow_box));

    let empty_state_label = Label::new(Some("Take a screenshot and it will appear here."));
    empty_state_label.add_css_class("history-empty-state");
    empty_state_label.set_halign(Align::Center);
    empty_state_label.set_valign(Align::Center);
    empty_state_label.set_visible(false);

    root.append(&header_row);
    root.append(&empty_state_label);
    root.append(&scroller);
    window.set_child(Some(&root));

    {
        let history_window = context.history_window.clone();
        window.connect_close_request(move |_| {
            history_window.borrow_mut().take();
            gtk4::glib::Propagation::Proceed
        });
    }

    let runtime = HistoryWindowRuntime {
        window,
        count_label,
        empty_state_label,
        flow_box,
    };
    runtime
}

fn close_history_window_if_open(context: &HistoryRenderContext) -> bool {
    let runtime = {
        let borrowed = context.history_window.borrow();
        borrowed.as_ref().cloned()
    };
    let Some(runtime) = runtime else {
        return false;
    };

    runtime.window.close();
    context.history_window.borrow_mut().take();
    true
}

fn close_duplicate_history_windows(app: &Application, keep: Option<&ApplicationWindow>) {
    let keep_ptr = keep.map(|window| window.as_ptr());

    for window in app.windows() {
        let title = window.title().map(|title| title.to_string());
        if title.as_deref() != Some(HISTORY_WINDOW_TITLE) {
            continue;
        }
        let Ok(window) = window.downcast::<ApplicationWindow>() else {
            continue;
        };
        if keep_ptr.is_some_and(|ptr| window.as_ptr() == ptr) {
            continue;
        }
        window.close();
    }
}

fn history_window_geometry(style_tokens: StyleTokens) -> Option<(i32, i32, i32, i32)> {
    let (monitor_x, monitor_y, monitor_width, monitor_height) =
        focused_monitor_geometry().or_else(primary_monitor_geometry)?;
    let margin = style_tokens.spacing_16.max(12);
    let width = HISTORY_WINDOW_WIDTH
        .min(monitor_width.saturating_sub(margin.saturating_mul(2)))
        .max(520);
    let height = HISTORY_WINDOW_HEIGHT
        .min(monitor_height.saturating_sub(margin.saturating_mul(2)))
        .max(420);
    let x = monitor_x
        .saturating_add(monitor_width)
        .saturating_sub(width)
        .saturating_sub(margin);
    let y = monitor_y.saturating_add(margin);
    Some((x, y, width, height))
}

fn primary_monitor_geometry() -> Option<(i32, i32, i32, i32)> {
    let display = gtk4::gdk::Display::default()?;
    let monitors = display.monitors();
    let item = monitors.item(0)?;
    let monitor = item.downcast::<gtk4::gdk::Monitor>().ok()?;
    let geometry = monitor.geometry();
    Some((
        geometry.x(),
        geometry.y(),
        geometry.width().max(1),
        geometry.height().max(1),
    ))
}

fn build_history_tile(
    context: &HistoryRenderContext,
    render: &Rc<dyn Fn()>,
    entry: &HistoryEntry,
) -> Frame {
    let tile = Frame::new(None);
    tile.add_css_class("history-tile");

    let tile_root = GtkBox::new(Orientation::Vertical, context.style_tokens.spacing_8);
    tile_root.set_margin_top(context.style_tokens.spacing_12);
    tile_root.set_margin_bottom(context.style_tokens.spacing_12);
    tile_root.set_margin_start(context.style_tokens.spacing_12);
    tile_root.set_margin_end(context.style_tokens.spacing_12);

    let preview_frame = Frame::new(None);
    preview_frame.add_css_class("history-thumbnail-frame");
    preview_frame.set_size_request(240, 152);

    let picture = Picture::for_file(&gtk4::gio::File::for_path(entry.display_thumbnail_path()));
    picture.set_keep_aspect_ratio(true);
    picture.set_can_shrink(true);
    picture.set_hexpand(true);
    picture.set_vexpand(true);
    preview_frame.set_child(Some(&picture));

    let title_label = Label::new(Some(&entry.capture_id));
    title_label.add_css_class("history-tile-title");
    title_label.set_halign(Align::Start);
    title_label.set_xalign(0.0);
    title_label.set_ellipsize(gtk4::pango::EllipsizeMode::End);

    let meta_label = Label::new(Some(&format!("{} x {}", entry.width, entry.height)));
    meta_label.add_css_class("history-tile-meta");
    meta_label.set_halign(Align::Start);
    meta_label.set_xalign(0.0);

    let saved_label = Label::new(Some(if entry.saved_path.is_some() {
        "Saved to screenshot folder"
    } else {
        "Not saved yet"
    }));
    saved_label.add_css_class("history-tile-saved");
    saved_label.set_halign(Align::Start);
    saved_label.set_xalign(0.0);

    let action_row = GtkBox::new(Orientation::Horizontal, context.style_tokens.spacing_8);

    let save_button = Button::with_label("Save");
    save_button.add_css_class("suggested-action");
    save_button.set_hexpand(true);
    save_button.set_tooltip_text(Some("Save to the screenshot folder"));

    let copy_button = Button::with_label("Copy");
    copy_button.set_hexpand(true);
    copy_button.set_tooltip_text(Some("Copy the image to the clipboard"));

    let edit_button = Button::with_label("Edit");
    edit_button.set_hexpand(true);
    edit_button.set_tooltip_text(Some("Open this capture in the editor"));

    action_row.append(&save_button);
    action_row.append(&copy_button);
    action_row.append(&edit_button);

    tile_root.append(&preview_frame);
    tile_root.append(&title_label);
    tile_root.append(&meta_label);
    tile_root.append(&saved_label);
    tile_root.append(&action_row);
    tile.set_child(Some(&tile_root));

    {
        let context = context.clone();
        let render = render.clone();
        let entry = entry.clone();
        save_button.connect_clicked(move |_| {
            save_history_entry(&context, &entry);
            (render.as_ref())();
        });
    }
    {
        let context = context.clone();
        let render = render.clone();
        let entry = entry.clone();
        copy_button.connect_clicked(move |_| {
            copy_history_entry(&context, &entry);
            (render.as_ref())();
        });
    }
    {
        let context = context.clone();
        let render = render.clone();
        let entry = entry.clone();
        edit_button.connect_clicked(move |_| {
            open_history_entry_in_editor(&context, &entry);
            (render.as_ref())();
        });
    }
    {
        let context = context.clone();
        let render = render.clone();
        let entry = entry.clone();
        let double_click = GestureClick::new();
        double_click.set_button(1);
        double_click.connect_pressed(move |_, n_press, _, _| {
            if n_press >= 2 {
                open_history_entry_in_editor(&context, &entry);
                (render.as_ref())();
            }
        });
        preview_frame.add_controller(double_click);
    }

    tile
}

fn clear_flow_box(flow_box: &FlowBox) {
    while let Some(child) = flow_box.first_child() {
        flow_box.remove(&child);
    }
}

fn save_history_entry(context: &HistoryRenderContext, entry: &HistoryEntry) {
    let Some(storage_service) = context.storage_service.as_ref().as_ref() else {
        *context.status_log.borrow_mut() = "storage service unavailable".to_string();
        return;
    };

    let artifact = entry.to_capture_artifact();
    match storage_service.save_capture(&artifact) {
        Ok(saved_path) => {
            if let Some(history_service) = context.history_service.as_ref().as_ref() {
                if let Err(err) = history_service.mark_saved(&entry.capture_id, &saved_path) {
                    tracing::warn!(
                        capture_id = %entry.capture_id,
                        ?err,
                        "failed to update history entry saved path"
                    );
                }
            }
            *context.status_log.borrow_mut() = format!("saved capture {}", entry.capture_id);
            notification::send(format!("Saved {}", entry.capture_id));
        }
        Err(err) => {
            *context.status_log.borrow_mut() =
                format!("save failed for {}: {err}", entry.capture_id);
            notification::send(format!("Save failed: {err}"));
        }
    }
}

fn copy_history_entry(context: &HistoryRenderContext, entry: &HistoryEntry) {
    match WlCopyBackend.copy(&entry.image_path) {
        Ok(()) => {
            *context.status_log.borrow_mut() = format!("copied capture {}", entry.capture_id);
            notification::send(format!("Copied {}", entry.capture_id));
        }
        Err(err) => {
            *context.status_log.borrow_mut() =
                format!("copy failed for {}: {err}", entry.capture_id);
            notification::send(format!("Copy failed: {err}"));
        }
    }
}

fn open_history_entry_in_editor(context: &HistoryRenderContext, entry: &HistoryEntry) {
    let state = context.shared_machine.borrow().state();
    if matches!(state, AppState::Editor) && *context.editor_has_unsaved_changes.borrow() {
        *context.status_log.borrow_mut() =
            "save or close the current editor session before opening another history item"
                .to_string();
        return;
    }

    let previous_capture_ids = context.runtime_session.borrow().ids_for_display();
    context
        .runtime_session
        .borrow_mut()
        .replace_with_capture(entry.to_capture_artifact());

    if let Some(storage_service) = context.storage_service.as_ref().as_ref() {
        for capture_id in previous_capture_ids {
            if capture_id == entry.capture_id {
                continue;
            }
            if let Err(err) = storage_service.discard_session_artifacts(&capture_id) {
                tracing::debug!(
                    capture_id = %capture_id,
                    ?err,
                    "failed to discard stale session artifact while opening history entry"
                );
            }
        }
    }

    match state {
        AppState::Idle => {
            if let Err(err) = context
                .shared_machine
                .borrow_mut()
                .transition(AppEvent::OpenPreview)
            {
                *context.status_log.borrow_mut() =
                    format!("cannot open preview for {}: {err}", entry.capture_id);
                return;
            }
            if let Err(err) = context
                .shared_machine
                .borrow_mut()
                .transition(AppEvent::OpenEditor)
            {
                *context.status_log.borrow_mut() =
                    format!("cannot open editor for {}: {err}", entry.capture_id);
                return;
            }
        }
        AppState::Preview => {
            if let Err(err) = context
                .shared_machine
                .borrow_mut()
                .transition(AppEvent::OpenEditor)
            {
                *context.status_log.borrow_mut() =
                    format!("cannot open editor for {}: {err}", entry.capture_id);
                return;
            }
        }
        AppState::Editor => {}
    }

    *context.status_log.borrow_mut() = format!("editor opened for {}", entry.capture_id);
}
