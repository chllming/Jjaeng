use std::cell::{Cell, RefCell};
use std::rc::Rc;

use crate::ui::{icon_button, icon_toggle_button, StyleTokens};
use gtk4::gdk::Key;
use gtk4::prelude::*;
use gtk4::{
    Align, Application, ApplicationWindow, Box as GtkBox, Button, ComboBoxText, EventControllerKey,
    Frame, Image, Label, Orientation, ToggleButton,
};
use jjaeng_core::identity::APP_CSS_ROOT;
use jjaeng_core::recording::{RecordingRequest, RecordingSelection, RecordingTarget};

use super::hypr::request_window_floating_with_geometry;
use super::layout::bottom_centered_window_geometry_for_point;
use super::recording_controls::{
    apply_audio_config_to_controls, connect_audio_toggle_controls,
    load_recording_source_availability, populate_microphone_combo,
    populate_recording_encoding_combo, populate_recording_size_combo, populate_system_audio_combo,
    recording_audio_config_from_controls, recording_encoding_from_combo, recording_size_from_combo,
    sync_audio_controls, RecordingSourceAvailability,
};
use super::window_state::RuntimeWindowGeometry;

const RECORDING_PROMPT_TITLE: &str = "Jjaeng Recording Controls";
const RECORDING_SELECTION_TITLE: &str = "Jjaeng Recording Selection";
const RECORDING_PROMPT_WIDTH: i32 = 920;
const RECORDING_PROMPT_HEIGHT: i32 = 188;

#[derive(Clone)]
pub(super) struct RecordingPromptRuntime {
    prompt_window: ApplicationWindow,
    outline_window: Option<ApplicationWindow>,
    close_guard: Rc<Cell<bool>>,
    active: Rc<Cell<bool>>,
    starting: Rc<Cell<bool>>,
    system_available: bool,
    microphone_available: bool,
    status_label: Label,
    timer_label: Label,
    hint_label: Label,
    size_combo: ComboBoxText,
    encoding_combo: ComboBoxText,
    system_toggle: ToggleButton,
    system_combo: ComboBoxText,
    mic_toggle: ToggleButton,
    mic_combo: ComboBoxText,
    start_button: Button,
    pause_button: Button,
    stop_button: Button,
    cancel_button: Button,
}

impl RecordingPromptRuntime {
    fn close(self) {
        self.close_guard.set(true);
        if let Some(outline_window) = self.outline_window {
            outline_window.close();
        }
        self.prompt_window.close();
    }

    fn set_controls_sensitive(&self, sensitive: bool) {
        let editable = sensitive && !self.active.get() && !self.starting.get();
        self.size_combo.set_sensitive(editable);
        self.encoding_combo.set_sensitive(editable);
        sync_audio_controls(
            &self.system_toggle,
            &self.system_combo,
            self.system_available,
            &self.mic_toggle,
            &self.mic_combo,
            self.microphone_available,
            editable,
        );
        self.start_button.set_sensitive(editable);
        self.pause_button
            .set_sensitive(sensitive && self.active.get() && !self.starting.get());
        self.stop_button
            .set_sensitive(sensitive && self.active.get() && !self.starting.get());
        self.cancel_button.set_sensitive(editable);
    }

    fn set_starting(&self) {
        self.starting.set(true);
        self.status_label.set_text("Starting recording...");
        self.set_controls_sensitive(false);
    }

    fn set_error(&self, message: &str) {
        self.starting.set(false);
        self.active.set(false);
        self.status_label.set_text(message);
        self.hint_label.set_text("Enter record  •  Esc cancel");
        self.start_button.set_visible(true);
        self.cancel_button.set_visible(true);
        self.pause_button.set_visible(false);
        self.stop_button.set_visible(false);
        self.set_controls_sensitive(true);
    }

    fn sync_state(&self, active: bool, paused: bool, elapsed_ms: u64) {
        self.active.set(active);
        self.starting.set(false);
        self.timer_label.set_text(&format_elapsed(elapsed_ms));
        self.start_button.set_visible(!active);
        self.cancel_button.set_visible(!active);
        self.pause_button.set_visible(active);
        self.stop_button.set_visible(active);
        if paused {
            self.pause_button
                .set_icon_name("media-playback-start-symbolic");
            self.pause_button.set_tooltip_text(Some("Resume recording"));
        } else {
            self.pause_button
                .set_icon_name("media-playback-pause-symbolic");
            self.pause_button.set_tooltip_text(Some("Pause recording"));
        }
        self.status_label.set_text(if active {
            if paused {
                "Recording paused"
            } else {
                "Recording live"
            }
        } else {
            "Review settings and press record"
        });
        self.hint_label.set_text(if active {
            "Space pause/resume  •  Esc stop"
        } else {
            "Enter record  •  Esc cancel"
        });
        self.set_controls_sensitive(true);
    }
}

pub(super) fn recording_prompt_open(
    recording_prompt: &Rc<RefCell<Option<RecordingPromptRuntime>>>,
) -> bool {
    recording_prompt.borrow().is_some()
}

pub(super) fn dismiss_recording_prompt(
    recording_prompt: &Rc<RefCell<Option<RecordingPromptRuntime>>>,
) {
    if let Some(runtime) = recording_prompt.borrow_mut().take() {
        runtime.close();
    }
}

pub(super) fn set_recording_prompt_starting(
    recording_prompt: &Rc<RefCell<Option<RecordingPromptRuntime>>>,
) {
    if let Some(runtime) = recording_prompt.borrow().as_ref() {
        runtime.set_starting();
    }
}

pub(super) fn set_recording_prompt_error(
    recording_prompt: &Rc<RefCell<Option<RecordingPromptRuntime>>>,
    message: &str,
) {
    if let Some(runtime) = recording_prompt.borrow().as_ref() {
        runtime.set_error(message);
    }
}

pub(super) fn sync_recording_prompt(
    recording_prompt: &Rc<RefCell<Option<RecordingPromptRuntime>>>,
    recording_active: bool,
    recording_paused: bool,
    elapsed_ms: u64,
) {
    if let Some(runtime) = recording_prompt.borrow().as_ref() {
        runtime.sync_state(recording_active, recording_paused, elapsed_ms);
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn present_recording_prompt(
    app: &Application,
    style_tokens: StyleTokens,
    recording_prompt: &Rc<RefCell<Option<RecordingPromptRuntime>>>,
    request: &RecordingRequest,
    selection: &RecordingSelection,
    source_availability: &RecordingSourceAvailability,
    on_start: &Rc<dyn Fn(RecordingRequest, RecordingSelection)>,
    on_cancel: &Rc<dyn Fn()>,
    on_pause_toggle: &Rc<dyn Fn()>,
    on_stop: &Rc<dyn Fn()>,
) {
    tracing::debug!(
        target = ?selection.target(),
        selection = ?selection,
        system_source_count = source_availability.system_sources.len(),
        microphone_source_count = source_availability.microphone_sources.len(),
        "presenting recording prompt"
    );
    dismiss_recording_prompt(recording_prompt);

    let outline_window = build_selection_outline_window(app, selection);
    if let Some(window) = outline_window.as_ref() {
        window.present();
        let geometry = selection.geometry();
        request_window_floating_with_geometry(
            "recording-selection",
            RECORDING_SELECTION_TITLE,
            true,
            Some((
                geometry.x,
                geometry.y,
                geometry.width as i32,
                geometry.height as i32,
            )),
            false,
            false,
            false,
        );
    }

    let prompt_window = ApplicationWindow::new(app);
    prompt_window.set_title(Some(RECORDING_PROMPT_TITLE));
    prompt_window.set_decorated(false);
    prompt_window.set_resizable(false);
    prompt_window.add_css_class(APP_CSS_ROOT);
    prompt_window.add_css_class("recording-prompt-window");

    let root = GtkBox::new(Orientation::Vertical, style_tokens.spacing_8);
    root.add_css_class("recording-prompt-surface");
    root.set_margin_top(style_tokens.spacing_12);
    root.set_margin_bottom(style_tokens.spacing_12);
    root.set_margin_start(style_tokens.spacing_12);
    root.set_margin_end(style_tokens.spacing_12);

    let title_row = GtkBox::new(Orientation::Horizontal, style_tokens.spacing_8);
    title_row.add_css_class("recording-prompt-header");

    let target_chip = recording_target_chip(selection);
    target_chip.set_hexpand(false);

    let status_label = Label::new(Some("Review settings and press record"));
    status_label.add_css_class("recording-prompt-status");
    status_label.set_halign(Align::Start);
    status_label.set_xalign(0.0);
    status_label.set_hexpand(true);

    let timer_label = Label::new(Some("00:00"));
    timer_label.add_css_class("recording-bar-timer");
    timer_label.set_halign(Align::End);
    timer_label.set_xalign(1.0);

    title_row.append(&target_chip);
    title_row.append(&status_label);
    title_row.append(&timer_label);

    let size_combo = ComboBoxText::new();
    populate_recording_size_combo(&size_combo, request.options.size);
    size_combo.set_size_request(96, -1);

    let encoding_combo = ComboBoxText::new();
    populate_recording_encoding_combo(&encoding_combo, request.options.encoding);
    encoding_combo.set_size_request(92, -1);

    let system_toggle = icon_toggle_button(
        "audio-volume-high-symbolic",
        "Capture system audio",
        style_tokens.control_size as i32,
        &["recording-bar-toggle"],
    );
    let system_combo = ComboBoxText::new();
    populate_system_audio_combo(
        &system_combo,
        &source_availability.system_sources,
        request.options.audio.system_device.as_deref(),
    );
    system_combo.set_size_request(196, -1);

    let mic_toggle = icon_toggle_button(
        "audio-input-microphone-symbolic",
        "Capture microphone audio",
        style_tokens.control_size as i32,
        &["recording-bar-toggle"],
    );
    let mic_combo = ComboBoxText::new();
    populate_microphone_combo(
        &mic_combo,
        &source_availability.microphone_sources,
        request.options.audio.microphone_device.as_deref(),
    );
    mic_combo.set_size_request(196, -1);

    let system_available = !source_availability.system_sources.is_empty();
    let microphone_available = !source_availability.microphone_sources.is_empty();
    connect_audio_toggle_controls(
        &system_toggle,
        &system_combo,
        system_available,
        &mic_toggle,
        &mic_combo,
        microphone_available,
    );
    apply_audio_config_to_controls(
        &system_toggle,
        &system_combo,
        system_available,
        &mic_toggle,
        &mic_combo,
        microphone_available,
        &request.options.audio,
    );

    let start_button = icon_button(
        "media-record-symbolic",
        "Start recording",
        style_tokens.control_size as i32,
        &["recording-bar-action", "recording-bar-record"],
    );
    let pause_button = icon_button(
        "media-playback-pause-symbolic",
        "Pause recording",
        style_tokens.control_size as i32,
        &["recording-bar-action"],
    );
    let stop_button = icon_button(
        "media-playback-stop-symbolic",
        "Stop recording",
        style_tokens.control_size as i32,
        &["recording-bar-action", "recording-bar-stop"],
    );
    let cancel_button = icon_button(
        "x-symbolic",
        "Cancel recording",
        style_tokens.control_size as i32,
        &["recording-bar-action"],
    );
    pause_button.set_visible(false);
    stop_button.set_visible(false);

    let control_bar = GtkBox::new(Orientation::Horizontal, style_tokens.spacing_8);
    control_bar.add_css_class("recording-prompt-bar");
    control_bar.append(&recording_bar_combo_segment(
        "move-up-right-symbolic",
        "Scale",
        &size_combo,
    ));
    control_bar.append(&recording_bar_toggle_segment(&system_toggle, &system_combo));
    control_bar.append(&recording_bar_toggle_segment(&mic_toggle, &mic_combo));
    control_bar.append(&recording_bar_combo_segment(
        "preferences-system-symbolic",
        "Quality",
        &encoding_combo,
    ));

    let action_bar = GtkBox::new(Orientation::Horizontal, style_tokens.spacing_4);
    action_bar.add_css_class("recording-bar-actions");
    action_bar.append(&cancel_button);
    action_bar.append(&start_button);
    action_bar.append(&pause_button);
    action_bar.append(&stop_button);
    control_bar.append(&action_bar);

    let hint_label = Label::new(Some("Enter record  •  Esc cancel"));
    hint_label.add_css_class("recording-prompt-hint");
    hint_label.set_halign(Align::Start);
    hint_label.set_xalign(0.0);
    hint_label.set_hexpand(true);

    let source_hint_label = Label::new(source_availability.source_hint().as_deref());
    source_hint_label.add_css_class("recording-prompt-hint");
    source_hint_label.add_css_class("recording-prompt-source-hint");
    source_hint_label.set_halign(Align::End);
    source_hint_label.set_xalign(1.0);
    source_hint_label.set_visible(source_availability.source_hint().is_some());

    let footer_row = GtkBox::new(Orientation::Horizontal, style_tokens.spacing_8);
    footer_row.add_css_class("recording-prompt-footer");
    footer_row.append(&hint_label);
    footer_row.append(&source_hint_label);

    root.append(&title_row);
    root.append(&control_bar);
    root.append(&footer_row);
    prompt_window.set_child(Some(&root));
    prompt_window.set_default_size(RECORDING_PROMPT_WIDTH, RECORDING_PROMPT_HEIGHT);
    prompt_window.set_size_request(RECORDING_PROMPT_WIDTH, RECORDING_PROMPT_HEIGHT);

    let close_guard = Rc::new(Cell::new(false));
    let active = Rc::new(Cell::new(false));
    let starting = Rc::new(Cell::new(false));

    {
        let close_guard = close_guard.clone();
        let active = active.clone();
        let on_cancel = on_cancel.clone();
        let on_stop = on_stop.clone();
        prompt_window.connect_close_request(move |_| {
            if close_guard.get() {
                return gtk4::glib::Propagation::Proceed;
            }
            if active.get() {
                (on_stop.as_ref())();
            } else {
                (on_cancel.as_ref())();
            }
            gtk4::glib::Propagation::Stop
        });
    }

    {
        let target = request.target;
        let size_combo = size_combo.clone();
        let encoding_combo = encoding_combo.clone();
        let system_toggle = system_toggle.clone();
        let system_combo = system_combo.clone();
        let mic_toggle = mic_toggle.clone();
        let mic_combo = mic_combo.clone();
        let selection = selection.clone();
        let on_start = on_start.clone();
        start_button.connect_clicked(move |_| {
            (on_start.as_ref())(
                recording_request_from_controls(
                    target,
                    &size_combo,
                    &encoding_combo,
                    &system_toggle,
                    &system_combo,
                    &mic_toggle,
                    &mic_combo,
                ),
                selection.clone(),
            );
        });
    }

    {
        let on_cancel = on_cancel.clone();
        cancel_button.connect_clicked(move |_| {
            (on_cancel.as_ref())();
        });
    }

    {
        let on_pause_toggle = on_pause_toggle.clone();
        pause_button.connect_clicked(move |_| {
            (on_pause_toggle.as_ref())();
        });
    }

    {
        let on_stop = on_stop.clone();
        stop_button.connect_clicked(move |_| {
            (on_stop.as_ref())();
        });
    }

    {
        let target = request.target;
        let active = active.clone();
        let size_combo = size_combo.clone();
        let encoding_combo = encoding_combo.clone();
        let system_toggle = system_toggle.clone();
        let system_combo = system_combo.clone();
        let mic_toggle = mic_toggle.clone();
        let mic_combo = mic_combo.clone();
        let selection = selection.clone();
        let on_start = on_start.clone();
        let on_cancel = on_cancel.clone();
        let on_pause_toggle = on_pause_toggle.clone();
        let on_stop = on_stop.clone();
        let key_controller = EventControllerKey::new();
        key_controller.connect_key_pressed(move |_, key, _, _| match key {
            Key::Escape => {
                if active.get() {
                    (on_stop.as_ref())();
                } else {
                    (on_cancel.as_ref())();
                }
                gtk4::glib::Propagation::Stop
            }
            Key::Return | Key::KP_Enter if !active.get() => {
                (on_start.as_ref())(
                    recording_request_from_controls(
                        target,
                        &size_combo,
                        &encoding_combo,
                        &system_toggle,
                        &system_combo,
                        &mic_toggle,
                        &mic_combo,
                    ),
                    selection.clone(),
                );
                gtk4::glib::Propagation::Stop
            }
            Key::space if active.get() => {
                (on_pause_toggle.as_ref())();
                gtk4::glib::Propagation::Stop
            }
            _ => gtk4::glib::Propagation::Proceed,
        });
        prompt_window.add_controller(key_controller);
    }

    prompt_window.present();
    let selection_geometry = selection.geometry();
    let prompt_geometry = bottom_centered_window_geometry_for_point(
        selection_geometry
            .x
            .saturating_add(selection_geometry.width as i32 / 2),
        selection_geometry
            .y
            .saturating_add(selection_geometry.height as i32 / 2),
        RuntimeWindowGeometry::new(RECORDING_PROMPT_WIDTH, RECORDING_PROMPT_HEIGHT),
        style_tokens.spacing_24,
    );
    request_window_floating_with_geometry(
        "recording-prompt",
        RECORDING_PROMPT_TITLE,
        true,
        Some(prompt_geometry),
        false,
        true,
        true,
    );

    let runtime = RecordingPromptRuntime {
        prompt_window,
        outline_window,
        close_guard,
        active,
        starting,
        system_available,
        microphone_available,
        status_label,
        timer_label,
        hint_label,
        size_combo,
        encoding_combo,
        system_toggle,
        system_combo,
        mic_toggle,
        mic_combo,
        start_button,
        pause_button,
        stop_button,
        cancel_button,
    };
    runtime.sync_state(false, false, 0);
    recording_prompt.borrow_mut().replace(runtime);
}

pub(super) fn current_recording_source_availability() -> RecordingSourceAvailability {
    load_recording_source_availability()
}

fn build_selection_outline_window(
    app: &Application,
    selection: &RecordingSelection,
) -> Option<ApplicationWindow> {
    if matches!(selection.target(), RecordingTarget::Fullscreen) {
        return None;
    }

    let geometry = selection.geometry();
    if geometry.width == 0 || geometry.height == 0 {
        return None;
    }

    let window = ApplicationWindow::new(app);
    window.set_title(Some(RECORDING_SELECTION_TITLE));
    window.set_decorated(false);
    window.set_resizable(false);
    window.add_css_class(APP_CSS_ROOT);
    window.add_css_class("recording-selection-window");
    window.set_default_size(geometry.width as i32, geometry.height as i32);
    window.set_size_request(geometry.width as i32, geometry.height as i32);

    let frame = Frame::new(None);
    frame.add_css_class("recording-selection-frame");
    window.set_child(Some(&frame));
    Some(window)
}

fn recording_target_chip(selection: &RecordingSelection) -> GtkBox {
    let chip = GtkBox::new(Orientation::Horizontal, 6);
    chip.add_css_class("recording-bar-chip");

    let icon = Image::from_icon_name(recording_target_icon_name(selection.target()));
    icon.set_pixel_size(16);

    let label = Label::new(Some(&selection_label(selection)));
    label.add_css_class("recording-bar-chip-label");
    label.set_halign(Align::Start);
    label.set_xalign(0.0);

    chip.append(&icon);
    chip.append(&label);
    chip
}

fn recording_bar_toggle_segment(toggle: &ToggleButton, combo: &ComboBoxText) -> GtkBox {
    let segment = GtkBox::new(Orientation::Horizontal, 6);
    segment.add_css_class("recording-bar-segment");
    combo.add_css_class("recording-bar-combo");
    segment.append(toggle);
    segment.append(combo);
    segment
}

fn recording_bar_combo_segment(icon_name: &str, tooltip: &str, combo: &ComboBoxText) -> GtkBox {
    let segment = GtkBox::new(Orientation::Horizontal, 6);
    segment.add_css_class("recording-bar-segment");

    let icon = Image::from_icon_name(icon_name);
    icon.set_pixel_size(16);
    icon.add_css_class("recording-bar-segment-icon");
    icon.set_tooltip_text(Some(tooltip));

    combo.add_css_class("recording-bar-combo");
    segment.append(&icon);
    segment.append(combo);
    segment
}

fn recording_target_icon_name(target: RecordingTarget) -> &'static str {
    match target {
        RecordingTarget::Fullscreen => "view-fullscreen-symbolic",
        RecordingTarget::Region => "crop-symbolic",
        RecordingTarget::Window => "scan-symbolic",
    }
}

fn selection_label(selection: &RecordingSelection) -> String {
    let geometry = selection.geometry();
    let target = match selection.target() {
        RecordingTarget::Fullscreen => "Full",
        RecordingTarget::Region => "Region",
        RecordingTarget::Window => "Window",
    };
    format!("{target} {} x {}", geometry.width, geometry.height)
}

fn recording_request_from_controls(
    target: RecordingTarget,
    size_combo: &ComboBoxText,
    encoding_combo: &ComboBoxText,
    system_toggle: &ToggleButton,
    system_combo: &ComboBoxText,
    mic_toggle: &ToggleButton,
    mic_combo: &ComboBoxText,
) -> RecordingRequest {
    let mut request = RecordingRequest::new(target);
    request.options.size = recording_size_from_combo(size_combo);
    request.options.encoding = recording_encoding_from_combo(encoding_combo);
    request.options.audio =
        recording_audio_config_from_controls(system_toggle, system_combo, mic_toggle, mic_combo);
    request
}

fn format_elapsed(elapsed_ms: u64) -> String {
    let total_seconds = elapsed_ms / 1000;
    let hours = total_seconds / 3600;
    let minutes = (total_seconds % 3600) / 60;
    let seconds = total_seconds % 60;
    if hours > 0 {
        format!("{hours:02}:{minutes:02}:{seconds:02}")
    } else {
        format!("{minutes:02}:{seconds:02}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_elapsed_switches_to_hours_after_sixty_minutes() {
        assert_eq!(format_elapsed(59_000), "00:59");
        assert_eq!(format_elapsed(3_661_000), "01:01:01");
    }
}
