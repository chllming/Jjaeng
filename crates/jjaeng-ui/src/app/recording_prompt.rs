use std::cell::{Cell, RefCell};
use std::rc::Rc;

use crate::ui::StyleTokens;
use gtk4::gdk::Key;
use gtk4::prelude::*;
use gtk4::{
    Align, Application, ApplicationWindow, Box as GtkBox, Button, ComboBoxText, EventControllerKey,
    Frame, Label, Orientation,
};
use jjaeng_core::identity::APP_CSS_ROOT;
use jjaeng_core::recording::{
    self, AudioMode, AudioSource, RecordingEncodingPreset, RecordingRequest, RecordingSelection,
    RecordingSize, RecordingTarget,
};

use super::hypr::request_window_floating_with_geometry;
use super::layout::bottom_centered_window_geometry_for_point;
use super::window_state::RuntimeWindowGeometry;

const RECORDING_PROMPT_TITLE: &str = "Jjaeng Recording Controls";
const RECORDING_SELECTION_TITLE: &str = "Jjaeng Recording Selection";
const RECORDING_PROMPT_WIDTH: i32 = 640;
const RECORDING_PROMPT_HEIGHT: i32 = 320;

#[derive(Clone)]
pub(super) struct RecordingPromptRuntime {
    prompt_window: ApplicationWindow,
    outline_window: Option<ApplicationWindow>,
    close_guard: Rc<Cell<bool>>,
    active: Rc<Cell<bool>>,
    starting: Rc<Cell<bool>>,
    status_label: Label,
    timer_label: Label,
    hint_label: Label,
    size_combo: ComboBoxText,
    encoding_combo: ComboBoxText,
    audio_combo: ComboBoxText,
    mic_combo: ComboBoxText,
    mic_row: GtkBox,
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
        self.size_combo
            .set_sensitive(sensitive && !self.active.get() && !self.starting.get());
        self.encoding_combo
            .set_sensitive(sensitive && !self.active.get() && !self.starting.get());
        self.audio_combo
            .set_sensitive(sensitive && !self.active.get() && !self.starting.get());
        self.mic_combo.set_sensitive(
            sensitive && !self.active.get() && !self.starting.get() && self.mic_row.is_visible(),
        );
        self.start_button
            .set_sensitive(sensitive && !self.active.get() && !self.starting.get());
        self.pause_button
            .set_sensitive(sensitive && self.active.get() && !self.starting.get());
        self.stop_button
            .set_sensitive(sensitive && self.active.get() && !self.starting.get());
        self.cancel_button
            .set_sensitive(sensitive && !self.active.get() && !self.starting.get());
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
        self.hint_label.set_text("Enter start  •  Esc cancel");
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
        self.pause_button
            .set_label(if paused { "Resume" } else { "Pause" });
        self.status_label.set_text(if active {
            if paused {
                "Recording paused"
            } else {
                "Recording live"
            }
        } else {
            "Review settings and press Start"
        });
        self.hint_label.set_text(if active {
            "Space pause/resume  •  Esc stop"
        } else {
            "Enter start  •  Esc cancel"
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
    microphone_sources: &[AudioSource],
    on_start: &Rc<dyn Fn(RecordingRequest, RecordingSelection)>,
    on_cancel: &Rc<dyn Fn()>,
    on_pause_toggle: &Rc<dyn Fn()>,
    on_stop: &Rc<dyn Fn()>,
) {
    tracing::debug!(
        target = ?selection.target(),
        selection = ?selection,
        microphone_source_count = microphone_sources.len(),
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

    let root = GtkBox::new(Orientation::Vertical, style_tokens.spacing_12);
    root.add_css_class("recording-prompt-surface");
    root.set_margin_top(style_tokens.spacing_12);
    root.set_margin_bottom(style_tokens.spacing_12);
    root.set_margin_start(style_tokens.spacing_12);
    root.set_margin_end(style_tokens.spacing_12);

    let title_label = Label::new(Some("Video Recording"));
    title_label.add_css_class("recording-prompt-title");
    title_label.set_halign(Align::Start);
    title_label.set_xalign(0.0);

    let timer_label = Label::new(Some("00:00"));
    timer_label.add_css_class("recording-prompt-timer");
    timer_label.set_halign(Align::End);
    timer_label.set_xalign(1.0);
    timer_label.set_hexpand(true);

    let title_row = GtkBox::new(Orientation::Horizontal, style_tokens.spacing_8);
    title_row.append(&title_label);
    title_row.append(&timer_label);

    let meta_label = Label::new(Some(&selection_label(selection)));
    meta_label.add_css_class("recording-prompt-meta");
    meta_label.set_halign(Align::Start);
    meta_label.set_xalign(0.0);
    meta_label.set_wrap(true);
    meta_label.set_wrap_mode(gtk4::pango::WrapMode::WordChar);
    meta_label.set_max_width_chars(48);

    let status_label = Label::new(Some("Review settings and press Start"));
    status_label.add_css_class("recording-prompt-status");
    status_label.set_halign(Align::Start);
    status_label.set_xalign(0.0);
    status_label.set_wrap(true);
    status_label.set_wrap_mode(gtk4::pango::WrapMode::WordChar);
    status_label.set_max_width_chars(48);

    let size_combo = ComboBoxText::new();
    size_combo.append(Some("native"), "Native");
    size_combo.append(Some("half"), "Half");
    size_combo.append(Some("fit1080p"), "1080p");
    size_combo.append(Some("fit720p"), "720p");
    size_combo.set_active_id(Some(match request.options.size {
        RecordingSize::Native => "native",
        RecordingSize::Half => "half",
        RecordingSize::Fit1080p => "fit1080p",
        RecordingSize::Fit720p => "fit720p",
    }));

    let encoding_combo = ComboBoxText::new();
    encoding_combo.append(Some("standard"), "Standard");
    encoding_combo.append(Some("quality"), "High Quality");
    encoding_combo.append(Some("small"), "Small File");
    encoding_combo.set_active_id(Some(match request.options.encoding {
        RecordingEncodingPreset::Standard => "standard",
        RecordingEncodingPreset::HighQuality => "quality",
        RecordingEncodingPreset::SmallFile => "small",
    }));

    let audio_combo = ComboBoxText::new();
    audio_combo.append(Some("off"), "No Audio");
    audio_combo.append(Some("desktop"), "Desktop");
    audio_combo.append(Some("microphone"), "Mic");
    audio_combo.set_active_id(Some(match request.options.audio.mode {
        AudioMode::Desktop => "desktop",
        AudioMode::Microphone => "microphone",
        AudioMode::Off => "off",
        AudioMode::Both => "desktop",
    }));

    let controls_column = GtkBox::new(Orientation::Vertical, style_tokens.spacing_8);
    controls_column.add_css_class("recording-prompt-controls");
    controls_column.append(&control_row(style_tokens, "Size", &size_combo));
    controls_column.append(&control_row(style_tokens, "Encoding", &encoding_combo));
    controls_column.append(&control_row(style_tokens, "Audio", &audio_combo));

    let mic_combo = ComboBoxText::new();
    populate_microphone_combo(
        &mic_combo,
        microphone_sources,
        request.options.audio.microphone_device.as_deref(),
    );
    mic_combo.set_hexpand(true);

    let mic_row = control_row(style_tokens, "Microphone", &mic_combo);
    mic_row.add_css_class("recording-prompt-mic-row");
    mic_row.set_visible(matches!(request.options.audio.mode, AudioMode::Microphone));

    {
        let mic_row = mic_row.clone();
        let mic_combo = mic_combo.clone();
        audio_combo.connect_changed(move |combo| {
            let visible = matches!(combo.active_id().as_deref(), Some("microphone"));
            mic_row.set_visible(visible);
            mic_combo.set_sensitive(visible);
        });
    }

    let hint_label = Label::new(Some("Enter start  •  Esc cancel"));
    hint_label.add_css_class("recording-prompt-hint");
    hint_label.set_halign(Align::Start);
    hint_label.set_xalign(0.0);
    hint_label.set_wrap(true);
    hint_label.set_wrap_mode(gtk4::pango::WrapMode::WordChar);
    hint_label.set_max_width_chars(48);

    let cancel_button = Button::with_label("Cancel");
    cancel_button.add_css_class("recording-prompt-button");
    cancel_button.set_hexpand(true);
    let start_button = Button::with_label("Start");
    start_button.add_css_class("recording-prompt-button");
    start_button.add_css_class("recording-prompt-button-primary");
    start_button.set_hexpand(true);
    let pause_button = Button::with_label("Pause");
    pause_button.add_css_class("recording-prompt-button");
    pause_button.set_hexpand(true);
    let stop_button = Button::with_label("Stop");
    stop_button.add_css_class("recording-prompt-button");
    stop_button.add_css_class("recording-prompt-button-danger");
    stop_button.set_hexpand(true);
    pause_button.set_visible(false);
    stop_button.set_visible(false);

    let button_row = GtkBox::new(Orientation::Horizontal, style_tokens.spacing_8);
    button_row.add_css_class("recording-prompt-button-row");
    button_row.set_homogeneous(true);
    button_row.append(&cancel_button);
    button_row.append(&start_button);
    button_row.append(&pause_button);
    button_row.append(&stop_button);

    root.append(&title_row);
    root.append(&meta_label);
    root.append(&status_label);
    root.append(&controls_column);
    root.append(&mic_row);
    root.append(&hint_label);
    root.append(&button_row);
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
        let audio_combo = audio_combo.clone();
        let mic_combo = mic_combo.clone();
        let selection = selection.clone();
        let on_start = on_start.clone();
        start_button.connect_clicked(move |_| {
            (on_start.as_ref())(
                recording_request_from_controls(
                    target,
                    &size_combo,
                    &encoding_combo,
                    &audio_combo,
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
        let audio_combo = audio_combo.clone();
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
                        &audio_combo,
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
        status_label,
        timer_label,
        hint_label,
        size_combo,
        encoding_combo,
        audio_combo,
        mic_combo,
        mic_row,
        start_button,
        pause_button,
        stop_button,
        cancel_button,
    };
    runtime.sync_state(false, false, 0);
    recording_prompt.borrow_mut().replace(runtime);
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

fn control_label(text: &str) -> Label {
    let label = Label::new(Some(text));
    label.add_css_class("recording-prompt-control-label");
    label.set_halign(Align::Start);
    label.set_xalign(0.0);
    label.set_width_chars(11);
    label
}

fn control_row<W: IsA<gtk4::Widget>>(
    style_tokens: StyleTokens,
    label_text: &str,
    control: &W,
) -> GtkBox {
    let row = GtkBox::new(Orientation::Horizontal, style_tokens.spacing_8);
    row.add_css_class("recording-prompt-control-row");
    control.set_hexpand(true);
    row.append(&control_label(label_text));
    row.append(control);
    row
}

fn selection_label(selection: &RecordingSelection) -> String {
    let geometry = selection.geometry();
    let target = match selection.target() {
        RecordingTarget::Fullscreen => "Fullscreen",
        RecordingTarget::Region => "Region",
        RecordingTarget::Window => "Window",
    };
    format!("{target} · {} x {}", geometry.width, geometry.height)
}

fn recording_request_from_controls(
    target: RecordingTarget,
    size_combo: &ComboBoxText,
    encoding_combo: &ComboBoxText,
    audio_combo: &ComboBoxText,
    mic_combo: &ComboBoxText,
) -> RecordingRequest {
    let mut request = RecordingRequest::new(target);
    request.options.size = match size_combo.active_id().as_deref() {
        Some("half") => RecordingSize::Half,
        Some("fit1080p") => RecordingSize::Fit1080p,
        Some("fit720p") => RecordingSize::Fit720p,
        _ => RecordingSize::Native,
    };
    request.options.encoding = match encoding_combo.active_id().as_deref() {
        Some("quality") => RecordingEncodingPreset::HighQuality,
        Some("small") => RecordingEncodingPreset::SmallFile,
        _ => RecordingEncodingPreset::Standard,
    };
    request.options.audio.mode = match audio_combo.active_id().as_deref() {
        Some("desktop") => AudioMode::Desktop,
        Some("microphone") => AudioMode::Microphone,
        _ => AudioMode::Off,
    };
    request.options.audio.microphone_device =
        if matches!(request.options.audio.mode, AudioMode::Microphone) {
            sanitize_microphone_device(mic_combo.active_id().map(|value| value.to_string()))
                .or_else(recording::default_microphone_source)
        } else {
            None
        };
    request
}

fn sanitize_microphone_device(value: Option<String>) -> Option<String> {
    value.and_then(|value| {
        let trimmed = value.trim();
        (!trimmed.is_empty()).then(|| trimmed.to_string())
    })
}

fn populate_microphone_combo(
    mic_combo: &ComboBoxText,
    microphone_sources: &[AudioSource],
    requested_device: Option<&str>,
) {
    for source in microphone_sources {
        mic_combo.append(Some(&source.name), &audio_source_label(source));
    }

    let fallback_device = recording::default_microphone_source();
    let selected_device = requested_device
        .map(str::to_string)
        .or(fallback_device)
        .or_else(|| microphone_sources.first().map(|source| source.name.clone()));

    if let Some(selected_device) = selected_device {
        if !microphone_sources
            .iter()
            .any(|source| source.name == selected_device)
        {
            mic_combo.append(Some(&selected_device), &selected_device);
        }
        mic_combo.set_active_id(Some(&selected_device));
    }

    if mic_combo.active_id().is_none() {
        mic_combo.append(Some(""), "No microphone sources detected");
        mic_combo.set_active_id(Some(""));
        mic_combo.set_sensitive(false);
    }
}

fn audio_source_label(source: &AudioSource) -> String {
    source
        .name
        .rsplit_once('.')
        .map(|(_, label)| label.replace('-', " "))
        .unwrap_or_else(|| source.name.clone())
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
    fn sanitize_microphone_device_discards_blank_values() {
        assert_eq!(sanitize_microphone_device(None), None);
        assert_eq!(sanitize_microphone_device(Some(String::new())), None);
        assert_eq!(sanitize_microphone_device(Some("   ".to_string())), None);
    }

    #[test]
    fn sanitize_microphone_device_trims_selected_value() {
        assert_eq!(
            sanitize_microphone_device(Some("  alsa_input.usb-mic  ".to_string())),
            Some("alsa_input.usb-mic".to_string())
        );
    }

    #[test]
    fn format_elapsed_switches_to_hours_after_sixty_minutes() {
        assert_eq!(format_elapsed(59_000), "00:59");
        assert_eq!(format_elapsed(3_661_000), "01:01:01");
    }
}
