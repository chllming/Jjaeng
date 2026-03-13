use std::cell::{Cell, RefCell};
use std::process::Command;
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
    filter: Rc<Cell<HistoryFilter>>,
    all_filter_button: Button,
    image_filter_button: Button,
    video_filter_button: Button,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum HistoryFilter {
    All,
    Images,
    Videos,
}

pub(super) fn present_history_window(context: &HistoryRenderContext, render: &Rc<dyn Fn()>) {
    let existing_runtime = { context.history_window.borrow().as_ref().cloned() };
    let runtime = if let Some(runtime) = existing_runtime {
        runtime
    } else {
        close_duplicate_history_windows(&context.app, None);
        let runtime = build_history_window(context, render);
        context.history_window.borrow_mut().replace(runtime.clone());
        runtime
    };

    close_duplicate_history_windows(&context.app, Some(&runtime.window));
    let (width, height) = history_window_size(context.style_tokens);
    runtime.window.set_default_size(width, height);
    runtime.window.set_size_request(width, height);
    request_window_floating_with_geometry(
        "history",
        HISTORY_WINDOW_TITLE,
        true,
        Some((0, 0, width, height)),
        true,
        true,
        false,
    );
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

    sync_history_filter_buttons(&runtime);
    let filtered_entries = entries
        .iter()
        .filter(|entry| history_filter_matches(runtime.filter.get(), entry))
        .cloned()
        .collect::<Vec<_>>();

    runtime
        .count_label
        .set_text(&if filtered_entries.len() == entries.len() {
            format!(
                "{} item{}",
                filtered_entries.len(),
                if filtered_entries.len() == 1 { "" } else { "s" }
            )
        } else {
            format!(
                "{} of {} item{}",
                filtered_entries.len(),
                entries.len(),
                if entries.len() == 1 { "" } else { "s" }
            )
        });
    runtime
        .empty_state_label
        .set_visible(filtered_entries.is_empty());
    if filtered_entries.is_empty() {
        runtime
            .empty_state_label
            .set_text(match runtime.filter.get() {
                HistoryFilter::All => "Take a screenshot or recording and it will appear here.",
                HistoryFilter::Images => "No screenshots in history yet.",
                HistoryFilter::Videos => "No recordings in history yet.",
            });
    }

    clear_flow_box(&runtime.flow_box);
    for entry in filtered_entries {
        let tile = build_history_tile(context, render, &entry);
        runtime.flow_box.insert(&tile, -1);
    }
}

fn build_history_window(
    context: &HistoryRenderContext,
    render: &Rc<dyn Fn()>,
) -> HistoryWindowRuntime {
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

    let kicker_label = Label::new(Some("Screenshots and recordings"));
    kicker_label.add_css_class("history-kicker");
    kicker_label.set_halign(Align::Start);
    kicker_label.set_xalign(0.0);

    let title_label = Label::new(Some("History"));
    title_label.add_css_class("history-title");
    title_label.set_halign(Align::Start);
    title_label.set_xalign(0.0);

    let subtitle_label = Label::new(Some("Filter, save, copy, or reopen recent history items."));
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

    let filter_row = GtkBox::new(Orientation::Horizontal, context.style_tokens.spacing_4);
    filter_row.set_halign(Align::Start);
    filter_row.add_css_class("history-filter-row");

    let all_filter_button = Button::with_label("All");
    all_filter_button.add_css_class("history-filter-button");
    let image_filter_button = Button::with_label("Images");
    image_filter_button.add_css_class("history-filter-button");
    let video_filter_button = Button::with_label("Videos");
    video_filter_button.add_css_class("history-filter-button");
    filter_row.append(&all_filter_button);
    filter_row.append(&image_filter_button);
    filter_row.append(&video_filter_button);

    meta_stack.append(&count_label);
    meta_stack.append(&shortcut_label);

    title_stack.append(&kicker_label);
    title_stack.append(&title_label);
    title_stack.append(&subtitle_label);
    title_stack.append(&filter_row);
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

    let empty_state_label = Label::new(Some(
        "Take a screenshot or recording and it will appear here.",
    ));
    empty_state_label.add_css_class("history-empty-state");
    empty_state_label.set_halign(Align::Center);
    empty_state_label.set_valign(Align::Center);
    empty_state_label.set_visible(false);

    root.append(&header_frame);
    root.append(&empty_state_label);
    root.append(&scroller);
    window.set_child(Some(&root));

    let filter = Rc::new(Cell::new(HistoryFilter::All));

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
    {
        let filter = filter.clone();
        let context = context.clone();
        let render = render.clone();
        let all_button = all_filter_button.clone();
        all_button.connect_clicked(move |_| {
            filter.set(HistoryFilter::All);
            refresh_history_window_if_open(&context, &render);
        });
    }
    {
        let filter = filter.clone();
        let context = context.clone();
        let render = render.clone();
        let image_button = image_filter_button.clone();
        image_button.connect_clicked(move |_| {
            filter.set(HistoryFilter::Images);
            refresh_history_window_if_open(&context, &render);
        });
    }
    {
        let filter = filter.clone();
        let context = context.clone();
        let render = render.clone();
        let video_button = video_filter_button.clone();
        video_button.connect_clicked(move |_| {
            filter.set(HistoryFilter::Videos);
            refresh_history_window_if_open(&context, &render);
        });
    }

    HistoryWindowRuntime {
        window,
        count_label,
        empty_state_label,
        flow_box,
        filter,
        all_filter_button,
        image_filter_button,
        video_filter_button,
    }
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

fn history_window_size(style_tokens: StyleTokens) -> (i32, i32) {
    focused_monitor_geometry()
        .or_else(primary_monitor_geometry)
        .map(|(_, _, monitor_width, monitor_height)| {
            history_window_size_for_monitor((monitor_width, monitor_height), style_tokens)
        })
        .unwrap_or((HISTORY_WINDOW_WIDTH, HISTORY_WINDOW_HEIGHT))
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

fn history_window_size_for_monitor(
    (monitor_width, monitor_height): (i32, i32),
    style_tokens: StyleTokens,
) -> (i32, i32) {
    let margin = style_tokens.spacing_16.max(12);
    let width = clamp_history_window_dimension(HISTORY_WINDOW_WIDTH, monitor_width, margin, 520);
    let height = clamp_history_window_dimension(HISTORY_WINDOW_HEIGHT, monitor_height, margin, 420);
    (width, height)
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

fn history_filter_matches(filter: HistoryFilter, entry: &HistoryEntry) -> bool {
    match filter {
        HistoryFilter::All => true,
        HistoryFilter::Images => entry.is_screenshot(),
        HistoryFilter::Videos => entry.is_recording(),
    }
}

fn sync_history_filter_buttons(runtime: &HistoryWindowRuntime) {
    for (button, active) in [
        (
            &runtime.all_filter_button,
            runtime.filter.get() == HistoryFilter::All,
        ),
        (
            &runtime.image_filter_button,
            runtime.filter.get() == HistoryFilter::Images,
        ),
        (
            &runtime.video_filter_button,
            runtime.filter.get() == HistoryFilter::Videos,
        ),
    ] {
        if active {
            button.add_css_class("history-filter-active");
        } else {
            button.remove_css_class("history-filter-active");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::LAYOUT_TOKENS;

    #[test]
    fn history_window_size_for_monitor_keeps_default_size_on_large_monitors() {
        assert_eq!(
            history_window_size_for_monitor((1920, 1080), LAYOUT_TOKENS),
            (HISTORY_WINDOW_WIDTH, HISTORY_WINDOW_HEIGHT)
        );
    }

    #[test]
    fn history_window_size_for_monitor_clamps_to_small_monitors() {
        assert_eq!(
            history_window_size_for_monitor((400, 300), LAYOUT_TOKENS),
            (368, 268)
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

    let media_badge = Label::new(Some(if entry.is_recording() {
        "VIDEO"
    } else {
        "IMAGE"
    }));
    media_badge.add_css_class("history-media-badge");
    if entry.is_recording() {
        media_badge.add_css_class("history-media-badge-video");
    } else {
        media_badge.add_css_class("history-media-badge-image");
    }
    media_badge.set_halign(Align::Start);
    media_badge.set_valign(Align::Start);
    media_badge.set_margin_top(context.style_tokens.spacing_8);
    media_badge.set_margin_start(context.style_tokens.spacing_8);

    let action_row = GtkBox::new(Orientation::Horizontal, context.style_tokens.spacing_4);
    action_row.set_halign(Align::End);
    action_row.add_css_class("history-action-row");

    let save_button = Button::with_label("Save");
    save_button.add_css_class("suggested-action");
    save_button.add_css_class("history-action-button");
    save_button.set_tooltip_text(Some(if entry.is_recording() {
        "Save to the videos folder"
    } else {
        "Save to the screenshot folder"
    }));

    let copy_button = Button::with_label(if entry.is_recording() {
        "Copy Path"
    } else {
        "Copy"
    });
    copy_button.add_css_class("history-action-button");
    copy_button.set_tooltip_text(Some(if entry.is_recording() {
        "Copy the recording path to the clipboard"
    } else {
        "Copy the image to the clipboard"
    }));

    let edit_button = Button::with_label("Open");
    edit_button.add_css_class("history-action-button");
    edit_button.set_tooltip_text(Some(if entry.is_recording() {
        "Open this recording"
    } else {
        "Open this capture in the editor"
    }));

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
    preview_overlay.add_overlay(&media_badge);
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

    let title_label = Label::new(Some(&history_capture_display_label(&entry.entry_id)));
    title_label.add_css_class("history-tile-title");
    title_label.set_halign(Align::Start);
    title_label.set_xalign(0.0);
    title_label.set_ellipsize(gtk4::pango::EllipsizeMode::End);
    title_label.set_tooltip_text(Some(&entry.entry_id));

    let meta_label = Label::new(Some(&history_entry_meta_label(entry)));
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
    } else if entry.is_recording() {
        "History copy"
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
            open_history_entry(&context, &entry);
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
                open_history_entry(&context, &entry);
                (render.as_ref())();
            }
        });
        picture.add_controller(double_click);
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

fn history_entry_meta_label(entry: &HistoryEntry) -> String {
    if entry.is_recording() {
        let duration = entry
            .duration_ms
            .map(format_duration_ms)
            .unwrap_or_else(|| "--:--".to_string());
        format!("{duration} · {} x {}", entry.width, entry.height)
    } else {
        format!("{} x {}", entry.width, entry.height)
    }
}

fn format_duration_ms(duration_ms: u64) -> String {
    let total_seconds = duration_ms / 1000;
    let minutes = total_seconds / 60;
    let seconds = total_seconds % 60;
    format!("{minutes:02}:{seconds:02}")
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

    let save_result = if let Some(artifact) = entry.to_capture_artifact() {
        storage_service.save_capture(&artifact)
    } else {
        let extension = entry.media_extension().unwrap_or("mp4");
        storage_service.save_recording_path(&entry.entry_id, &entry.media_path, extension)
    };

    match save_result {
        Ok(saved_path) => {
            if let Some(history_service) = context.history_service.as_ref().as_ref() {
                if let Err(err) = history_service.mark_saved(&entry.entry_id, &saved_path) {
                    tracing::warn!(
                        entry_id = %entry.entry_id,
                        ?err,
                        "failed to update history entry saved path"
                    );
                }
            }
            *context.status_log.borrow_mut() = format!("saved {}", entry.entry_id);
            notification::send(format!("Saved {}", entry.entry_id));
        }
        Err(err) => {
            *context.status_log.borrow_mut() = format!("save failed for {}: {err}", entry.entry_id);
            notification::send(format!("Save failed: {err}"));
        }
    }
}

fn copy_history_entry(context: &HistoryRenderContext, entry: &HistoryEntry) {
    match WlCopyBackend.copy(&entry.media_path) {
        Ok(()) => {
            *context.status_log.borrow_mut() = format!("copied {}", entry.entry_id);
            notification::send(format!("Copied {}", entry.entry_id));
        }
        Err(err) => {
            *context.status_log.borrow_mut() = format!("copy failed for {}: {err}", entry.entry_id);
            notification::send(format!("Copy failed: {err}"));
        }
    }
}

fn open_history_entry(context: &HistoryRenderContext, entry: &HistoryEntry) {
    if entry.is_recording() {
        match Command::new("xdg-open").arg(&entry.media_path).spawn() {
            Ok(_) => {
                *context.status_log.borrow_mut() = format!("opened {}", entry.entry_id);
            }
            Err(err) => {
                *context.status_log.borrow_mut() =
                    format!("open failed for {}: {err}", entry.entry_id);
                notification::send(format!("Open failed: {err}"));
            }
        }
        return;
    }

    let state = context.shared_machine.borrow().state();
    if matches!(state, AppState::Editor) && *context.editor_has_unsaved_changes.borrow() {
        *context.status_log.borrow_mut() =
            "save or close the current editor session before opening another history item"
                .to_string();
        return;
    }

    let previous_capture_ids = context.runtime_session.borrow().ids_for_display();
    context.runtime_session.borrow_mut().replace_with_capture(
        entry
            .to_capture_artifact()
            .expect("history screenshots should convert to capture artifact"),
    );

    if let Some(storage_service) = context.storage_service.as_ref().as_ref() {
        for capture_id in previous_capture_ids {
            if capture_id == entry.entry_id {
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
                    format!("cannot open preview for {}: {err}", entry.entry_id);
                return;
            }
            if let Err(err) = context
                .shared_machine
                .borrow_mut()
                .transition(AppEvent::OpenEditor)
            {
                *context.status_log.borrow_mut() =
                    format!("cannot open editor for {}: {err}", entry.entry_id);
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
                    format!("cannot open editor for {}: {err}", entry.entry_id);
                return;
            }
        }
        AppState::Editor => {}
        AppState::Recording => {
            *context.status_log.borrow_mut() =
                "stop recording before opening screenshots in the editor".to_string();
            return;
        }
    }

    *context.status_log.borrow_mut() = format!("editor opened for {}", entry.entry_id);
}
