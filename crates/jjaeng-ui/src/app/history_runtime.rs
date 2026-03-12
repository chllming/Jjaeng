use std::cell::RefCell;
use std::rc::Rc;

use gtk4::prelude::*;
use gtk4::{
    Align, Application, ApplicationWindow, Box as GtkBox, Button, EventControllerKey,
    EventControllerMotion, FlowBox, Frame, GestureClick, Label, Orientation, Overlay, Picture,
    PolicyType, Revealer, RevealerTransitionType, ScrolledWindow, SelectionMode,
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
const HISTORY_WINDOW_WIDTH: i32 = 875;
const HISTORY_WINDOW_HEIGHT: i32 = 600;

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
            true,
            true,
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

    let header_frame = Frame::new(None);
    header_frame.add_css_class("history-header-card");

    let header_row = GtkBox::new(Orientation::Horizontal, context.style_tokens.spacing_12);
    header_row.set_halign(Align::Fill);
    header_row.set_margin_top(context.style_tokens.spacing_16);
    header_row.set_margin_bottom(context.style_tokens.spacing_16);
    header_row.set_margin_start(context.style_tokens.spacing_16);
    header_row.set_margin_end(context.style_tokens.spacing_16);

    let title_stack = GtkBox::new(Orientation::Vertical, context.style_tokens.spacing_4);
    title_stack.set_hexpand(true);
    title_stack.add_css_class("history-title-stack");

    let kicker_label = Label::new(Some("Screenshot archive"));
    kicker_label.add_css_class("history-kicker");
    kicker_label.set_halign(Align::Start);
    kicker_label.set_xalign(0.0);

    let title_label = Label::new(Some("History"));
    title_label.add_css_class("history-title");
    title_label.set_halign(Align::Start);
    title_label.set_xalign(0.0);

    let subtitle_label = Label::new(Some("Copy, save, or reopen recent captures."));
    subtitle_label.add_css_class("history-subtitle");
    subtitle_label.set_halign(Align::Start);
    subtitle_label.set_xalign(0.0);

    let meta_stack = GtkBox::new(Orientation::Vertical, context.style_tokens.spacing_4);
    meta_stack.set_halign(Align::End);
    meta_stack.set_valign(Align::Start);

    let count_label = Label::new(Some("0 items"));
    count_label.add_css_class("history-count");
    count_label.set_halign(Align::End);
    count_label.set_valign(Align::Start);

    let shortcut_label = Label::new(Some("Close: Esc or Super+F5"));
    shortcut_label.add_css_class("history-shortcut-tip");
    shortcut_label.set_halign(Align::End);
    shortcut_label.set_xalign(1.0);

    meta_stack.append(&count_label);
    meta_stack.append(&shortcut_label);

    title_stack.append(&kicker_label);
    title_stack.append(&title_label);
    title_stack.append(&subtitle_label);
    header_row.append(&title_stack);
    header_row.append(&meta_stack);
    header_frame.set_child(Some(&header_row));

    let flow_box = FlowBox::new();
    flow_box.set_selection_mode(SelectionMode::None);
    flow_box.set_column_spacing(context.style_tokens.spacing_12 as u32);
    flow_box.set_row_spacing(context.style_tokens.spacing_12 as u32);
    flow_box.set_max_children_per_line(3);
    flow_box.set_homogeneous(true);
    flow_box.set_activate_on_single_click(false);
    flow_box.add_css_class("history-grid");

    let scroller = ScrolledWindow::new();
    scroller.set_policy(PolicyType::Never, PolicyType::Automatic);
    scroller.set_vexpand(true);
    scroller.set_hexpand(true);
    scroller.add_css_class("history-scroller");
    scroller.set_child(Some(&flow_box));

    let empty_state_label = Label::new(Some("Take a screenshot and it will appear here."));
    empty_state_label.add_css_class("history-empty-state");
    empty_state_label.set_halign(Align::Center);
    empty_state_label.set_valign(Align::Center);
    empty_state_label.set_visible(false);

    root.append(&header_frame);
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
    {
        let close_window = window.clone();
        let key_controller = EventControllerKey::new();
        key_controller.connect_key_pressed(move |_, key, _, _| {
            if key == gtk4::gdk::Key::Escape {
                close_window.close();
                return gtk4::glib::Propagation::Stop;
            }
            gtk4::glib::Propagation::Proceed
        });
        window.add_controller(key_controller);
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
    let monitor_geometry = focused_monitor_geometry().or_else(primary_monitor_geometry)?;
    Some(history_window_geometry_for_monitor(
        monitor_geometry,
        style_tokens,
    ))
}

fn clamp_history_window_dimension(
    target: i32,
    monitor_dimension: i32,
    margin: i32,
    minimum: i32,
) -> i32 {
    let available = monitor_dimension
        .saturating_sub(margin.saturating_mul(2))
        .max(1);
    if available >= minimum {
        target.min(available).max(minimum)
    } else {
        available
    }
}

fn history_window_geometry_for_monitor(
    (monitor_x, monitor_y, monitor_width, monitor_height): (i32, i32, i32, i32),
    style_tokens: StyleTokens,
) -> (i32, i32, i32, i32) {
    let margin = style_tokens.spacing_16.max(12);
    let width = clamp_history_window_dimension(HISTORY_WINDOW_WIDTH, monitor_width, margin, 520);
    let height = clamp_history_window_dimension(HISTORY_WINDOW_HEIGHT, monitor_height, margin, 420);
    let x = monitor_x.saturating_add((monitor_width.saturating_sub(width)).max(0) / 2);
    let y = monitor_y.saturating_add((monitor_height.saturating_sub(height)).max(0) / 2);
    (x, y, width, height)
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::LAYOUT_TOKENS;

    #[test]
    fn history_window_geometry_for_monitor_centers_on_focused_monitor() {
        assert_eq!(
            history_window_geometry_for_monitor((0, 0, 1920, 1080), LAYOUT_TOKENS),
            (522, 240, HISTORY_WINDOW_WIDTH, HISTORY_WINDOW_HEIGHT)
        );
    }

    #[test]
    fn history_window_geometry_for_monitor_clamps_to_small_monitors() {
        assert_eq!(
            history_window_geometry_for_monitor((0, 0, 400, 300), LAYOUT_TOKENS),
            (16, 16, 368, 268)
        );
    }
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

    let action_row = GtkBox::new(Orientation::Horizontal, context.style_tokens.spacing_4);
    action_row.set_halign(Align::End);
    action_row.add_css_class("history-action-row");

    let save_button = Button::with_label("Save");
    save_button.add_css_class("suggested-action");
    save_button.add_css_class("history-action-button");
    save_button.set_tooltip_text(Some("Save to the screenshot folder"));

    let copy_button = Button::with_label("Copy");
    copy_button.add_css_class("history-action-button");
    copy_button.set_tooltip_text(Some("Copy the image to the clipboard"));

    let edit_button = Button::with_label("Open");
    edit_button.add_css_class("history-action-button");
    edit_button.set_tooltip_text(Some("Open this capture in the editor"));

    action_row.append(&save_button);
    action_row.append(&copy_button);
    action_row.append(&edit_button);

    let action_revealer = Revealer::new();
    action_revealer.add_css_class("history-action-revealer");
    action_revealer.set_transition_type(RevealerTransitionType::SlideUp);
    action_revealer.set_transition_duration(140);
    action_revealer.set_halign(Align::Fill);
    action_revealer.set_valign(Align::End);
    action_revealer.set_reveal_child(false);

    let action_anchor = GtkBox::new(Orientation::Vertical, 0);
    action_anchor.set_halign(Align::Fill);
    action_anchor.set_valign(Align::End);
    action_anchor.set_margin_top(context.style_tokens.spacing_8);
    action_anchor.set_margin_bottom(context.style_tokens.spacing_8);
    action_anchor.set_margin_start(context.style_tokens.spacing_8);
    action_anchor.set_margin_end(context.style_tokens.spacing_8);
    action_anchor.append(&action_row);
    action_revealer.set_child(Some(&action_anchor));

    let preview_overlay = Overlay::new();
    preview_overlay.set_child(Some(&picture));
    preview_overlay.add_overlay(&action_revealer);
    preview_frame.set_child(Some(&preview_overlay));

    {
        let hover_controller = EventControllerMotion::new();
        let action_revealer_for_enter = action_revealer.clone();
        hover_controller.connect_enter(move |_, _, _| {
            action_revealer_for_enter.set_reveal_child(true);
        });
        let action_revealer_for_leave = action_revealer.clone();
        hover_controller.connect_leave(move |_| {
            action_revealer_for_leave.set_reveal_child(false);
        });
        preview_overlay.add_controller(hover_controller);
    }

    let title_label = Label::new(Some(&history_capture_display_label(&entry.capture_id)));
    title_label.add_css_class("history-tile-title");
    title_label.set_halign(Align::Start);
    title_label.set_xalign(0.0);
    title_label.set_ellipsize(gtk4::pango::EllipsizeMode::End);
    title_label.set_tooltip_text(Some(&entry.capture_id));

    let meta_label = Label::new(Some(&format!("{} x {}", entry.width, entry.height)));
    meta_label.add_css_class("history-tile-meta");
    meta_label.set_halign(Align::Start);
    meta_label.set_xalign(0.0);
    meta_label.set_hexpand(true);

    let info_row = GtkBox::new(Orientation::Horizontal, context.style_tokens.spacing_8);
    info_row.set_halign(Align::Fill);
    info_row.set_hexpand(true);
    info_row.add_css_class("history-info-row");

    let saved_label = Label::new(Some(if entry.saved_path.is_some() {
        "Saved"
    } else {
        "Session only"
    }));
    saved_label.add_css_class("history-status-chip");
    saved_label.add_css_class(if entry.saved_path.is_some() {
        "history-status-saved"
    } else {
        "history-status-unsaved"
    });
    saved_label.set_halign(Align::End);
    saved_label.set_xalign(1.0);

    info_row.append(&meta_label);
    info_row.append(&saved_label);

    tile_root.append(&preview_frame);
    tile_root.append(&title_label);
    tile_root.append(&info_row);
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

fn history_capture_display_label(capture_id: &str) -> String {
    const PREFIX_LEN: usize = 16;
    const SUFFIX_LEN: usize = 6;

    if capture_id.len() <= PREFIX_LEN + SUFFIX_LEN + 1 {
        return capture_id.to_string();
    }

    format!(
        "{}…{}",
        &capture_id[..PREFIX_LEN],
        &capture_id[capture_id.len() - SUFFIX_LEN..]
    )
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
