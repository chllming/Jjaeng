use std::cell::{Cell, RefCell};
use std::rc::Rc;

use crate::ui::{icon_button, icon_toggle_button, StyleTokens};
use gtk4::gdk::Key;
use gtk4::prelude::*;
use gtk4::{
    Align, Application, ApplicationWindow, ArrowType, Box as GtkBox, Button, ComboBoxText,
    EventControllerKey, Frame, Image, Label, MenuButton, Orientation, Popover, ToggleButton,
};
use jjaeng_core::identity::APP_CSS_ROOT;
use jjaeng_core::recording::{RecordingRequest, RecordingSelection, RecordingTarget};

use super::hypr::request_window_floating_with_geometry;
use super::layout::adjacent_window_geometry_for_area;
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
const RECORDING_PROMPT_WIDTH: i32 = 468;
const RECORDING_PROMPT_HEIGHT: i32 = 48;
const RECORDING_PROMPT_CONTROL_SIZE: i32 = 30;
const RECORDING_PROMPT_MENU_WIDTH: i32 = 24;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum RecordingPromptKeyAction {
    Cancel,
    Stop,
    Start,
    PauseToggle,
}

#[derive(Clone)]
pub(super) struct RecordingPromptRuntime {
    prompt_window: ApplicationWindow,
    outline_window: Option<ApplicationWindow>,
    close_guard: Rc<Cell<bool>>,
    active: Rc<Cell<bool>>,
    starting: Rc<Cell<bool>>,
    system_available: bool,
    microphone_available: bool,
    timer_label: Label,
    size_combo: ComboBoxText,
    size_menu_button: MenuButton,
    encoding_combo: ComboBoxText,
    encoding_menu_button: MenuButton,
    system_toggle: ToggleButton,
    system_combo: ComboBoxText,
    system_menu_button: MenuButton,
    mic_toggle: ToggleButton,
    mic_combo: ComboBoxText,
    mic_menu_button: MenuButton,
    start_button: Button,
    pause_button: Button,
    stop_button: Button,
    cancel_button: Button,
}

impl RecordingPromptRuntime {
    fn set_status_tooltip(&self, message: &str) {
        self.prompt_window.set_tooltip_text(Some(message));
        self.timer_label.set_tooltip_text(Some(message));
    }

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
        self.size_menu_button.set_sensitive(editable);
        self.encoding_combo.set_sensitive(editable);
        self.encoding_menu_button.set_sensitive(editable);
        sync_audio_controls(
            &self.system_toggle,
            &self.system_combo,
            self.system_available,
            &self.mic_toggle,
            &self.mic_combo,
            self.microphone_available,
            editable,
        );
        self.system_menu_button
            .set_sensitive(editable && self.system_available);
        self.mic_menu_button
            .set_sensitive(editable && self.microphone_available);
        self.start_button.set_sensitive(editable);
        self.pause_button
            .set_sensitive(sensitive && self.active.get() && !self.starting.get());
        self.stop_button
            .set_sensitive(sensitive && self.active.get() && !self.starting.get());
        self.cancel_button.set_sensitive(editable);
    }

    fn set_starting(&self) {
        self.starting.set(true);
        self.set_status_tooltip("Starting recording");
        self.set_controls_sensitive(false);
    }

    fn set_error(&self, message: &str) {
        self.starting.set(false);
        self.active.set(false);
        self.set_status_tooltip(message);
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
        self.set_status_tooltip(if active {
            if paused {
                "Recording paused. Press Space to resume or Esc to stop."
            } else {
                "Recording live. Press Space to pause or Esc to stop."
            }
        } else {
            "Review recording controls. Press Enter to record or Esc to cancel."
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

    let root = GtkBox::new(Orientation::Horizontal, 3);
    root.add_css_class("recording-prompt-surface");
    root.add_css_class("recording-prompt-bar");
    root.add_css_class("recording-prompt-compact");
    root.set_halign(Align::Center);
    root.set_valign(Align::Center);
    root.set_margin_top(6);
    root.set_margin_bottom(6);
    root.set_margin_start(6);
    root.set_margin_end(6);

    let target_indicator = recording_target_indicator(selection, RECORDING_PROMPT_CONTROL_SIZE);

    let timer_label = Label::new(Some("00:00"));
    timer_label.add_css_class("recording-bar-timer");
    timer_label.set_tooltip_text(Some("Recording timer"));

    let size_combo = ComboBoxText::new();
    populate_recording_size_combo(&size_combo, request.options.size);
    let size_menu_button = recording_option_menu_button(
        &size_combo,
        "Scale options",
        112,
        RECORDING_PROMPT_CONTROL_SIZE,
        RECORDING_PROMPT_MENU_WIDTH,
    );
    bind_combo_menu_tooltip(&size_menu_button, &size_combo, "Scale");

    let encoding_combo = ComboBoxText::new();
    populate_recording_encoding_combo(&encoding_combo, request.options.encoding);
    let encoding_menu_button = recording_option_menu_button(
        &encoding_combo,
        "Quality options",
        112,
        RECORDING_PROMPT_CONTROL_SIZE,
        RECORDING_PROMPT_MENU_WIDTH,
    );
    bind_combo_menu_tooltip(&encoding_menu_button, &encoding_combo, "Quality");

    let system_toggle = icon_toggle_button(
        "audio-volume-high-symbolic",
        "Capture system audio",
        RECORDING_PROMPT_CONTROL_SIZE,
        &["recording-bar-toggle"],
    );
    let system_combo = ComboBoxText::new();
    populate_system_audio_combo(
        &system_combo,
        &source_availability.system_sources,
        request.options.audio.system_device.as_deref(),
    );
    let system_menu_button = recording_option_menu_button(
        &system_combo,
        "System audio source",
        224,
        RECORDING_PROMPT_CONTROL_SIZE,
        RECORDING_PROMPT_MENU_WIDTH,
    );
    if let Some(message) = source_availability.system_error.as_ref() {
        system_menu_button.set_tooltip_text(Some(&format!("System audio unavailable: {message}")));
    } else {
        bind_combo_menu_tooltip(&system_menu_button, &system_combo, "System audio");
    }

    let mic_toggle = icon_toggle_button(
        "audio-input-microphone-symbolic",
        "Capture microphone audio",
        RECORDING_PROMPT_CONTROL_SIZE,
        &["recording-bar-toggle"],
    );
    let mic_combo = ComboBoxText::new();
    populate_microphone_combo(
        &mic_combo,
        &source_availability.microphone_sources,
        request.options.audio.microphone_device.as_deref(),
    );
    let mic_menu_button = recording_option_menu_button(
        &mic_combo,
        "Microphone source",
        224,
        RECORDING_PROMPT_CONTROL_SIZE,
        RECORDING_PROMPT_MENU_WIDTH,
    );
    if let Some(message) = source_availability.microphone_error.as_ref() {
        mic_menu_button.set_tooltip_text(Some(&format!("Microphone unavailable: {message}")));
    } else {
        bind_combo_menu_tooltip(&mic_menu_button, &mic_combo, "Microphone");
    }

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
        RECORDING_PROMPT_CONTROL_SIZE,
        &["recording-bar-action", "recording-bar-record"],
    );
    let pause_button = icon_button(
        "media-playback-pause-symbolic",
        "Pause recording",
        RECORDING_PROMPT_CONTROL_SIZE,
        &["recording-bar-action"],
    );
    let stop_button = icon_button(
        "media-playback-stop-symbolic",
        "Stop recording",
        RECORDING_PROMPT_CONTROL_SIZE,
        &["recording-bar-action", "recording-bar-stop"],
    );
    let cancel_button = icon_button(
        "x-symbolic",
        "Cancel recording",
        RECORDING_PROMPT_CONTROL_SIZE,
        &["recording-bar-action"],
    );
    pause_button.set_visible(false);
    stop_button.set_visible(false);

    let control_bar = GtkBox::new(Orientation::Horizontal, 6);
    control_bar.add_css_class("recording-prompt-bar");
    control_bar.set_halign(Align::Center);
    control_bar.append(&target_indicator);
    control_bar.append(&recording_bar_toggle_segment(
        &system_toggle,
        &system_menu_button,
    ));
    control_bar.append(&recording_bar_toggle_segment(&mic_toggle, &mic_menu_button));
    control_bar.append(&recording_bar_combo_segment(
        "move-up-right-symbolic",
        "Scale",
        &size_menu_button,
    ));
    control_bar.append(&recording_bar_combo_segment(
        "preferences-system-symbolic",
        "Quality",
        &encoding_menu_button,
    ));
    control_bar.append(&timer_label);

    let action_bar = GtkBox::new(Orientation::Horizontal, 6);
    action_bar.add_css_class("recording-bar-actions");
    action_bar.append(&cancel_button);
    action_bar.append(&start_button);
    action_bar.append(&pause_button);
    action_bar.append(&stop_button);
    control_bar.append(&action_bar);

    root.append(&control_bar);
    prompt_window.set_child(Some(&root));
    prompt_window.set_default_size(RECORDING_PROMPT_WIDTH, RECORDING_PROMPT_HEIGHT);
    prompt_window.set_size_request(RECORDING_PROMPT_WIDTH, RECORDING_PROMPT_HEIGHT);
    let initial_prompt_tooltip = source_availability.source_hint().unwrap_or_else(|| {
        "Review recording controls. Press Enter to record or Esc to cancel.".to_string()
    });
    prompt_window.set_tooltip_text(Some(&initial_prompt_tooltip));

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
            dispatch_recording_prompt_escape(&active, &on_cancel, &on_stop);
            gtk4::glib::Propagation::Stop
        });
    }

    if let Some(outline_window) = outline_window.as_ref() {
        let close_guard = close_guard.clone();
        let active = active.clone();
        let on_cancel = on_cancel.clone();
        let on_stop = on_stop.clone();
        outline_window.connect_close_request(move |_| {
            if close_guard.get() {
                return gtk4::glib::Propagation::Proceed;
            }
            dispatch_recording_prompt_escape(&active, &on_cancel, &on_stop);
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
        key_controller.connect_key_pressed(move |_, key, _, _| {
            match resolve_recording_prompt_key_action(key, active.get()) {
                Some(RecordingPromptKeyAction::Cancel) => {
                    (on_cancel.as_ref())();
                    gtk4::glib::Propagation::Stop
                }
                Some(RecordingPromptKeyAction::Stop) => {
                    (on_stop.as_ref())();
                    gtk4::glib::Propagation::Stop
                }
                Some(RecordingPromptKeyAction::Start) => {
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
                Some(RecordingPromptKeyAction::PauseToggle) => {
                    (on_pause_toggle.as_ref())();
                    gtk4::glib::Propagation::Stop
                }
                None => gtk4::glib::Propagation::Proceed,
            }
        });
        prompt_window.add_controller(key_controller);
    }

    if let Some(outline_window) = outline_window.as_ref() {
        let active = active.clone();
        let on_cancel = on_cancel.clone();
        let on_stop = on_stop.clone();
        let key_controller = EventControllerKey::new();
        key_controller.connect_key_pressed(move |_, key, _, _| {
            match resolve_recording_prompt_key_action(key, active.get()) {
                Some(RecordingPromptKeyAction::Cancel) => {
                    (on_cancel.as_ref())();
                    gtk4::glib::Propagation::Stop
                }
                Some(RecordingPromptKeyAction::Stop) => {
                    (on_stop.as_ref())();
                    gtk4::glib::Propagation::Stop
                }
                _ => gtk4::glib::Propagation::Proceed,
            }
        });
        outline_window.add_controller(key_controller);
    }

    prompt_window.present();
    let selection_geometry = selection.geometry();
    let prompt_geometry = adjacent_window_geometry_for_area(
        selection_geometry.x,
        selection_geometry.y,
        selection_geometry.width as i32,
        selection_geometry.height as i32,
        RuntimeWindowGeometry::new(RECORDING_PROMPT_WIDTH, RECORDING_PROMPT_HEIGHT),
        style_tokens.spacing_12,
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
        timer_label,
        size_combo,
        size_menu_button,
        encoding_combo,
        encoding_menu_button,
        system_toggle,
        system_combo,
        system_menu_button,
        mic_toggle,
        mic_combo,
        mic_menu_button,
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

fn resolve_recording_prompt_key_action(key: Key, active: bool) -> Option<RecordingPromptKeyAction> {
    match key {
        Key::Escape => Some(if active {
            RecordingPromptKeyAction::Stop
        } else {
            RecordingPromptKeyAction::Cancel
        }),
        Key::Return | Key::KP_Enter if !active => Some(RecordingPromptKeyAction::Start),
        Key::space if active => Some(RecordingPromptKeyAction::PauseToggle),
        _ => None,
    }
}

fn dispatch_recording_prompt_escape(
    active: &Rc<Cell<bool>>,
    on_cancel: &Rc<dyn Fn()>,
    on_stop: &Rc<dyn Fn()>,
) {
    match resolve_recording_prompt_key_action(Key::Escape, active.get()) {
        Some(RecordingPromptKeyAction::Cancel) => (on_cancel.as_ref())(),
        Some(RecordingPromptKeyAction::Stop) => (on_stop.as_ref())(),
        _ => unreachable!("escape must resolve to a prompt action"),
    }
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

fn recording_target_indicator(selection: &RecordingSelection, control_size: i32) -> GtkBox {
    recording_bar_icon_indicator_segment(
        recording_target_icon_name(selection.target()),
        &selection_label(selection),
        control_size,
    )
}

fn recording_bar_toggle_segment(toggle: &ToggleButton, menu_button: &MenuButton) -> GtkBox {
    let segment = GtkBox::new(Orientation::Horizontal, 2);
    segment.add_css_class("recording-bar-segment");
    segment.append(toggle);
    segment.append(menu_button);
    segment
}

fn recording_bar_combo_segment(icon_name: &str, tooltip: &str, menu_button: &MenuButton) -> GtkBox {
    let segment = GtkBox::new(Orientation::Horizontal, 2);
    segment.add_css_class("recording-bar-segment");

    let icon = Image::from_icon_name(icon_name);
    icon.set_pixel_size(16);
    icon.add_css_class("recording-bar-segment-icon");
    icon.set_tooltip_text(Some(tooltip));

    segment.append(&icon);
    segment.append(menu_button);
    segment
}

fn recording_bar_icon_indicator_segment(
    icon_name: &str,
    tooltip: &str,
    control_size: i32,
) -> GtkBox {
    let segment = GtkBox::new(Orientation::Horizontal, 0);
    segment.add_css_class("recording-bar-segment");

    let icon_box = GtkBox::new(Orientation::Horizontal, 0);
    icon_box.add_css_class("recording-bar-static-icon");
    icon_box.set_size_request(control_size, control_size);
    icon_box.set_halign(Align::Center);
    icon_box.set_valign(Align::Center);
    icon_box.set_tooltip_text(Some(tooltip));

    let icon = Image::from_icon_name(icon_name);
    icon.set_pixel_size(16);
    icon.add_css_class("recording-bar-segment-icon");
    icon_box.append(&icon);

    segment.append(&icon_box);
    segment
}

fn recording_option_menu_button(
    combo: &ComboBoxText,
    tooltip: &str,
    combo_width: i32,
    control_size: i32,
    menu_width: i32,
) -> MenuButton {
    combo.add_css_class("recording-popover-combo");
    combo.set_size_request(combo_width, -1);

    let popover = Popover::new();
    popover.add_css_class("recording-bar-popover");
    popover.set_has_arrow(false);
    let content = GtkBox::new(Orientation::Vertical, 0);
    content.set_margin_top(8);
    content.set_margin_bottom(8);
    content.set_margin_start(8);
    content.set_margin_end(8);
    content.append(combo);
    popover.set_child(Some(&content));

    let menu_button = MenuButton::new();
    menu_button.set_icon_name("chevron-down-symbolic");
    menu_button.set_direction(ArrowType::Down);
    menu_button.set_has_frame(true);
    menu_button.set_tooltip_text(Some(tooltip));
    menu_button.add_css_class("flat");
    menu_button.add_css_class("recording-bar-menu");
    menu_button.set_size_request(menu_width.max(1), control_size);
    menu_button.set_popover(Some(&popover));

    let popover = popover.clone();
    combo.connect_changed(move |_| {
        popover.popdown();
    });

    menu_button
}

fn bind_combo_menu_tooltip(menu_button: &MenuButton, combo: &ComboBoxText, prefix: &str) {
    update_combo_menu_tooltip(menu_button, combo, prefix);

    let menu_button = menu_button.clone();
    let prefix = prefix.to_string();
    combo.clone().connect_changed(move |combo| {
        update_combo_menu_tooltip(&menu_button, combo, &prefix);
    });
}

fn update_combo_menu_tooltip(menu_button: &MenuButton, combo: &ComboBoxText, prefix: &str) {
    let value = combo
        .active_text()
        .map(|text| text.to_string())
        .unwrap_or_else(|| "Unavailable".to_string());
    menu_button.set_tooltip_text(Some(&format!("{prefix}: {value}")));
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

    #[test]
    fn resolve_recording_prompt_key_action_uses_escape_to_cancel_when_idle() {
        assert_eq!(
            resolve_recording_prompt_key_action(Key::Escape, false),
            Some(RecordingPromptKeyAction::Cancel)
        );
    }

    #[test]
    fn resolve_recording_prompt_key_action_uses_escape_to_stop_when_active() {
        assert_eq!(
            resolve_recording_prompt_key_action(Key::Escape, true),
            Some(RecordingPromptKeyAction::Stop)
        );
    }
}
