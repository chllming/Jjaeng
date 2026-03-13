use std::rc::Rc;

use crate::ui::{icon_button, icon_toggle_button, StyleTokens};
use gtk4::prelude::*;
use gtk4::{
    Align, Box as GtkBox, Button, ComboBoxText, Frame, Image, Label, Orientation, ScrolledWindow,
    ToggleButton,
};
use jjaeng_core::capture;
use jjaeng_core::preview::PreviewAction;
use jjaeng_core::recording::{
    self, AudioConfig, AudioMode, RecordingEncodingPreset, RecordingRequest, RecordingSize,
    RecordingTarget,
};
use jjaeng_core::state::AppState;

use super::launchpad_actions::LaunchpadActionExecutor;
use super::recording_controls::{
    apply_audio_config_to_controls, connect_audio_toggle_controls,
    load_recording_source_availability, populate_microphone_combo,
    populate_recording_encoding_combo, populate_recording_size_combo, populate_system_audio_combo,
    recording_audio_config_from_controls, recording_encoding_from_combo, recording_size_from_combo,
    sync_audio_controls,
};

#[derive(Clone)]
pub(super) struct LaunchpadUi {
    pub(super) root: GtkBox,
    pub(super) toast_label: Label,
    pub(super) state_label: Label,
    pub(super) status_label: Label,
    pub(super) active_capture_label: Label,
    pub(super) capture_count_label: Label,
    pub(super) latest_label: Label,
    pub(super) capture_ids_label: Label,
    pub(super) full_capture_button: Button,
    pub(super) region_capture_button: Button,
    pub(super) window_capture_button: Button,
    pub(super) record_target_full_button: ToggleButton,
    pub(super) record_target_region_button: ToggleButton,
    pub(super) record_target_window_button: ToggleButton,
    pub(super) record_button: Button,
    pub(super) pause_recording_button: Button,
    pub(super) stop_recording_button: Button,
    pub(super) record_size_combo: ComboBoxText,
    pub(super) record_encoding_combo: ComboBoxText,
    pub(super) record_system_toggle: ToggleButton,
    pub(super) record_system_combo: ComboBoxText,
    pub(super) record_mic_toggle: ToggleButton,
    pub(super) record_mic_combo: ComboBoxText,
    pub(super) recording_system_available: bool,
    pub(super) recording_mic_available: bool,
    pub(super) recording_backend_label: Label,
    pub(super) recording_status_label: Label,
    pub(super) recording_timer_label: Label,
    pub(super) history_button: Button,
    pub(super) open_preview_button: Button,
    pub(super) open_editor_button: Button,
    pub(super) close_preview_button: Button,
    pub(super) close_editor_button: Button,
    pub(super) save_button: Button,
    pub(super) copy_button: Button,
    pub(super) ocr_button: Button,
    pub(super) delete_button: Button,
}

impl LaunchpadUi {
    pub(super) fn update_overview(
        &self,
        state: AppState,
        active_capture_id: &str,
        latest_capture_label: &str,
        ids: &[String],
    ) {
        self.state_label.set_text(&format!("{:?}", state));
        self.active_capture_label.set_text(active_capture_id);
        self.capture_count_label.set_text(&format!("{}", ids.len()));
        self.latest_label.set_text(latest_capture_label);
        self.capture_ids_label
            .set_text(&format_capture_ids_for_display(ids));
    }

    pub(super) fn set_action_availability(
        &self,
        state: AppState,
        has_capture: bool,
        ocr_available: bool,
        recording_available: bool,
    ) {
        let idle = matches!(state, AppState::Idle);
        let recording = matches!(state, AppState::Recording);
        let recording_idle = idle && recording_available;
        let recording_tooltip =
            (!recording_available).then(recording::recording_backend_requirement_message);
        self.full_capture_button.set_sensitive(idle);
        self.region_capture_button.set_sensitive(idle);
        self.window_capture_button.set_sensitive(idle);
        self.record_target_full_button.set_sensitive(recording_idle);
        self.record_target_region_button
            .set_sensitive(recording_idle);
        self.record_target_window_button
            .set_sensitive(recording_idle);
        self.record_button.set_sensitive(recording_idle);
        self.pause_recording_button.set_sensitive(recording);
        self.stop_recording_button.set_sensitive(recording);
        self.record_size_combo.set_sensitive(recording_idle);
        self.record_encoding_combo.set_sensitive(recording_idle);
        sync_audio_controls(
            &self.record_system_toggle,
            &self.record_system_combo,
            self.recording_system_available,
            &self.record_mic_toggle,
            &self.record_mic_combo,
            self.recording_mic_available,
            recording_idle,
        );
        self.record_target_full_button
            .set_tooltip_text(recording_tooltip.as_deref());
        self.record_target_region_button
            .set_tooltip_text(recording_tooltip.as_deref());
        self.record_target_window_button
            .set_tooltip_text(recording_tooltip.as_deref());
        self.record_button
            .set_tooltip_text(recording_tooltip.as_deref().or(Some("Start recording")));
        self.pause_recording_button
            .set_tooltip_text(recording_tooltip.as_deref().or(Some("Pause recording")));
        self.stop_recording_button
            .set_tooltip_text(recording_tooltip.as_deref());
        self.record_size_combo
            .set_tooltip_text(recording_tooltip.as_deref());
        self.record_encoding_combo
            .set_tooltip_text(recording_tooltip.as_deref());
        self.record_system_toggle.set_tooltip_text(
            recording_tooltip
                .as_deref()
                .or(Some("Capture system audio")),
        );
        self.record_system_combo
            .set_tooltip_text(recording_tooltip.as_deref());
        self.record_mic_toggle.set_tooltip_text(
            recording_tooltip
                .as_deref()
                .or(Some("Capture microphone audio")),
        );
        self.record_mic_combo
            .set_tooltip_text(recording_tooltip.as_deref());
        self.history_button.set_sensitive(true);
        self.open_preview_button
            .set_sensitive(matches!(state, AppState::Idle) && has_capture);
        self.open_editor_button
            .set_sensitive(matches!(state, AppState::Preview) && has_capture);
        self.close_preview_button
            .set_sensitive(matches!(state, AppState::Preview));
        self.close_editor_button
            .set_sensitive(matches!(state, AppState::Editor));
        self.save_button
            .set_sensitive(matches!(state, AppState::Preview) && has_capture);
        self.copy_button
            .set_sensitive(matches!(state, AppState::Preview) && has_capture);
        self.ocr_button
            .set_sensitive(ocr_available && matches!(state, AppState::Preview) && has_capture);
        self.ocr_button
            .set_tooltip_text((!ocr_available).then_some("OCR models not installed"));
        self.delete_button
            .set_sensitive(matches!(state, AppState::Preview) && has_capture);
    }

    pub(super) fn set_status_text(&self, message: &str) {
        self.status_label.set_text(message);
    }

    pub(super) fn update_recording_overview(
        &self,
        recording_available: bool,
        recording_active: bool,
        recording_paused: bool,
        elapsed_ms: u64,
    ) {
        let backend_label = if recording_available {
            format!("{} ready", recording::preferred_recording_backend_name())
        } else {
            "recorder missing".to_string()
        };
        let elapsed_label = if recording_active {
            format_recording_elapsed(elapsed_ms)
        } else {
            "00:00".to_string()
        };
        self.recording_backend_label.set_text(&backend_label);
        self.recording_status_label
            .set_text(if !recording_available {
                "Install a recorder"
            } else if recording_active {
                if recording_paused {
                    "Paused"
                } else {
                    "Live"
                }
            } else {
                "Ready"
            });
        self.recording_timer_label.set_text(&elapsed_label);
        if recording_paused {
            self.pause_recording_button
                .set_icon_name("media-playback-start-symbolic");
            self.pause_recording_button
                .set_tooltip_text(Some("Resume recording"));
        } else {
            self.pause_recording_button
                .set_icon_name("media-playback-pause-symbolic");
            self.pause_recording_button
                .set_tooltip_text(Some("Pause recording"));
        }
    }

    pub(super) fn recording_request(&self) -> RecordingRequest {
        let mut request = RecordingRequest::new(self.selected_recording_target());
        request.options.size = recording_size_from_combo(&self.record_size_combo);
        request.options.encoding = recording_encoding_from_combo(&self.record_encoding_combo);
        request.options.audio = recording_audio_config_from_controls(
            &self.record_system_toggle,
            &self.record_system_combo,
            &self.record_mic_toggle,
            &self.record_mic_combo,
        );
        request
    }

    fn selected_recording_target(&self) -> RecordingTarget {
        if self.record_target_window_button.is_active() {
            RecordingTarget::Window
        } else if self.record_target_full_button.is_active() {
            RecordingTarget::Fullscreen
        } else {
            RecordingTarget::Region
        }
    }
}

pub(super) fn launchpad_kv_row(key: &str, value_label: &Label) -> GtkBox {
    let key_label = Label::new(Some(key));
    key_label.add_css_class("launchpad-kv-key");
    key_label.set_halign(Align::Start);
    key_label.set_xalign(0.0);

    value_label.add_css_class("launchpad-kv-value");
    value_label.set_halign(Align::Start);
    value_label.set_xalign(0.0);
    value_label.set_hexpand(true);
    value_label.set_ellipsize(gtk4::pango::EllipsizeMode::End);

    let row = GtkBox::new(Orientation::Horizontal, 8);
    row.append(&key_label);
    row.append(value_label);
    row
}

pub(super) fn launchpad_kv_static(key: &str, value: &str) -> GtkBox {
    let value_label = Label::new(Some(value));
    launchpad_kv_row(key, &value_label)
}

pub(super) fn launchpad_section_title(text: &str) -> Label {
    let label = Label::new(Some(text));
    label.add_css_class("launchpad-section-title");
    label.set_halign(Align::Start);
    label.set_xalign(0.0);
    label
}

pub(super) fn launchpad_panel(style_tokens: StyleTokens, title: &str, child: &GtkBox) -> Frame {
    let panel = Frame::new(None);
    panel.add_css_class("launchpad-panel");
    let panel_box = GtkBox::new(Orientation::Vertical, style_tokens.spacing_8);
    panel_box.append(&launchpad_section_title(title));
    panel_box.append(child);
    panel.set_child(Some(&panel_box));
    panel
}

fn recording_target_button(icon_name: &str, tooltip: &str, control_size: i32) -> ToggleButton {
    let button = icon_toggle_button(
        icon_name,
        tooltip,
        control_size,
        &["recording-bar-toggle", "recording-target-button"],
    );
    button.set_can_focus(false);
    button
}

fn connect_recording_target_group(
    full_button: &ToggleButton,
    region_button: &ToggleButton,
    window_button: &ToggleButton,
) {
    {
        let full_button = full_button.clone();
        let region_button = region_button.clone();
        let window_button = window_button.clone();
        full_button.connect_toggled(move |toggle| {
            if toggle.is_active() {
                region_button.set_active(false);
                window_button.set_active(false);
            } else if !region_button.is_active() && !window_button.is_active() {
                toggle.set_active(true);
            }
        });
    }
    {
        let full_button = full_button.clone();
        let region_button = region_button.clone();
        let window_button = window_button.clone();
        region_button.connect_toggled(move |toggle| {
            if toggle.is_active() {
                full_button.set_active(false);
                window_button.set_active(false);
            } else if !full_button.is_active() && !window_button.is_active() {
                toggle.set_active(true);
            }
        });
    }
    {
        let full_button = full_button.clone();
        let region_button = region_button.clone();
        let window_button = window_button.clone();
        window_button.connect_toggled(move |toggle| {
            if toggle.is_active() {
                full_button.set_active(false);
                region_button.set_active(false);
            } else if !full_button.is_active() && !region_button.is_active() {
                toggle.set_active(true);
            }
        });
    }
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

pub(super) fn format_capture_ids_for_display(ids: &[String]) -> String {
    if ids.is_empty() {
        return "IDs: none".to_string();
    }

    let id_lines = ids
        .iter()
        .enumerate()
        .map(|(index, capture_id)| format!("{:>2}. {capture_id}", index + 1))
        .collect::<Vec<_>>()
        .join("\n");
    format!("IDs:\n{id_lines}")
}

fn format_recording_elapsed(elapsed_ms: u64) -> String {
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

pub(super) struct LaunchpadSettingsInfo {
    pub(super) theme_label: String,
    pub(super) ocr_language_label: String,
    pub(super) ocr_model_dir_label: String,
    pub(super) config_path: String,
    pub(super) theme_config_path: String,
    pub(super) keybinding_config_path: String,
}

#[derive(Debug, Clone)]
pub(super) struct LaunchpadRecordingDefaults {
    pub(super) size: RecordingSize,
    pub(super) encoding: RecordingEncodingPreset,
    pub(super) audio_mode: AudioMode,
    pub(super) system_device: Option<String>,
    pub(super) microphone_device: Option<String>,
}

pub(super) fn build_launchpad_ui(
    style_tokens: StyleTokens,
    show_launchpad: bool,
    settings_info: &LaunchpadSettingsInfo,
    recording_defaults: LaunchpadRecordingDefaults,
) -> LaunchpadUi {
    let root = GtkBox::new(Orientation::Vertical, style_tokens.spacing_12);
    root.set_margin_top(style_tokens.spacing_12);
    root.set_margin_bottom(style_tokens.spacing_12);
    root.set_margin_start(style_tokens.spacing_12);
    root.set_margin_end(style_tokens.spacing_12);
    root.add_css_class("launchpad-root");

    let toast_label = Label::new(Some(""));
    toast_label.add_css_class("toast-badge");
    toast_label.set_halign(Align::Start);
    toast_label.set_visible(false);

    // ── Header row: title + version badge ──
    let title_label = Label::new(Some(jjaeng_core::identity::APP_LAUNCHPAD_TITLE));
    title_label.add_css_class("launchpad-title");
    title_label.set_halign(Align::Start);
    title_label.set_xalign(0.0);

    let version_label = Label::new(Some(env!("CARGO_PKG_VERSION")));
    version_label.add_css_class("launchpad-version");
    version_label.set_halign(Align::Start);
    version_label.set_valign(Align::Center);

    let header_row = GtkBox::new(Orientation::Horizontal, style_tokens.spacing_8);
    header_row.append(&title_label);
    header_row.append(&version_label);

    let subtitle_label = Label::new(Some(
        "Quick control panel for validating capture, preview, and editor flow.",
    ));
    subtitle_label.add_css_class("launchpad-subtitle");
    subtitle_label.set_halign(Align::Start);
    subtitle_label.set_xalign(0.0);
    subtitle_label.set_wrap(true);

    // ── Capture panel (3 buttons, horizontal) ──
    let full_capture_button = Button::with_label("Full Capture");
    full_capture_button.add_css_class("launchpad-primary-button");
    full_capture_button.set_hexpand(true);
    let region_capture_button = Button::with_label("Region Capture");
    region_capture_button.set_hexpand(true);
    let window_capture_button = Button::with_label("Window Capture");
    window_capture_button.set_hexpand(true);
    let capture_row = GtkBox::new(Orientation::Horizontal, style_tokens.spacing_8);
    capture_row.append(&full_capture_button);
    capture_row.append(&region_capture_button);
    capture_row.append(&window_capture_button);
    let capture_panel = launchpad_panel(style_tokens, "Capture", &capture_row);

    let recording_source_availability = load_recording_source_availability();
    let recording_available = recording::recording_backend_available();

    let record_target_full_button = recording_target_button(
        "view-fullscreen-symbolic",
        "Record the focused monitor",
        style_tokens.control_size as i32,
    );
    let record_target_region_button = recording_target_button(
        "crop-symbolic",
        "Select a region to record",
        style_tokens.control_size as i32,
    );
    let record_target_window_button = recording_target_button(
        "scan-symbolic",
        "Select a window to record",
        style_tokens.control_size as i32,
    );
    record_target_region_button.set_active(true);
    connect_recording_target_group(
        &record_target_full_button,
        &record_target_region_button,
        &record_target_window_button,
    );

    let record_system_toggle = icon_toggle_button(
        "audio-volume-high-symbolic",
        "Capture system audio",
        style_tokens.control_size as i32,
        &["recording-bar-toggle"],
    );
    let record_system_combo = ComboBoxText::new();
    populate_system_audio_combo(
        &record_system_combo,
        &recording_source_availability.system_sources,
        recording_defaults.system_device.as_deref(),
    );
    record_system_combo.set_size_request(196, -1);

    let record_mic_toggle = icon_toggle_button(
        "audio-input-microphone-symbolic",
        "Capture microphone audio",
        style_tokens.control_size as i32,
        &["recording-bar-toggle"],
    );
    let record_mic_combo = ComboBoxText::new();
    populate_microphone_combo(
        &record_mic_combo,
        &recording_source_availability.microphone_sources,
        recording_defaults.microphone_device.as_deref(),
    );
    record_mic_combo.set_size_request(196, -1);

    let record_size_combo = ComboBoxText::new();
    populate_recording_size_combo(&record_size_combo, recording_defaults.size);
    record_size_combo.set_size_request(96, -1);

    let record_encoding_combo = ComboBoxText::new();
    populate_recording_encoding_combo(&record_encoding_combo, recording_defaults.encoding);
    record_encoding_combo.set_size_request(92, -1);

    let recording_audio_defaults = AudioConfig {
        mode: recording_defaults.audio_mode,
        microphone_device: recording_defaults.microphone_device.clone(),
        system_device: recording_defaults.system_device.clone(),
    };
    let recording_system_available = !recording_source_availability.system_sources.is_empty();
    let recording_mic_available = !recording_source_availability.microphone_sources.is_empty();
    connect_audio_toggle_controls(
        &record_system_toggle,
        &record_system_combo,
        recording_system_available,
        &record_mic_toggle,
        &record_mic_combo,
        recording_mic_available,
    );
    apply_audio_config_to_controls(
        &record_system_toggle,
        &record_system_combo,
        recording_system_available,
        &record_mic_toggle,
        &record_mic_combo,
        recording_mic_available,
        &recording_audio_defaults,
    );

    let record_button = icon_button(
        "media-record-symbolic",
        "Start recording",
        style_tokens.control_size as i32,
        &["recording-bar-action", "recording-bar-record"],
    );
    let pause_recording_button = icon_button(
        "media-playback-pause-symbolic",
        "Pause recording",
        style_tokens.control_size as i32,
        &["recording-bar-action"],
    );
    let stop_recording_button = icon_button(
        "media-playback-stop-symbolic",
        "Stop recording",
        style_tokens.control_size as i32,
        &["recording-bar-action", "recording-bar-stop"],
    );

    let recording_backend_text = if recording_available {
        format!("{} ready", recording::preferred_recording_backend_name())
    } else {
        "recorder missing".to_string()
    };
    let recording_backend_label = Label::new(Some(&recording_backend_text));
    recording_backend_label.add_css_class("recording-meta-chip");
    let recording_status_label = Label::new(Some(if recording_available {
        "Ready"
    } else {
        "Install a recorder"
    }));
    recording_status_label.add_css_class("recording-meta-chip");
    let recording_timer_label = Label::new(Some("00:00"));
    recording_timer_label.add_css_class("recording-bar-timer");

    let recording_bar = GtkBox::new(Orientation::Horizontal, style_tokens.spacing_8);
    recording_bar.add_css_class("launchpad-recording-bar");
    recording_bar.append(&record_target_full_button);
    recording_bar.append(&record_target_region_button);
    recording_bar.append(&record_target_window_button);
    recording_bar.append(&recording_bar_toggle_segment(
        &record_system_toggle,
        &record_system_combo,
    ));
    recording_bar.append(&recording_bar_toggle_segment(
        &record_mic_toggle,
        &record_mic_combo,
    ));
    recording_bar.append(&recording_bar_combo_segment(
        "move-up-right-symbolic",
        "Scale",
        &record_size_combo,
    ));
    recording_bar.append(&recording_bar_combo_segment(
        "preferences-system-symbolic",
        "Quality",
        &record_encoding_combo,
    ));

    let recording_action_bar = GtkBox::new(Orientation::Horizontal, style_tokens.spacing_4);
    recording_action_bar.add_css_class("recording-bar-actions");
    recording_action_bar.append(&recording_timer_label);
    recording_action_bar.append(&record_button);
    recording_action_bar.append(&pause_recording_button);
    recording_action_bar.append(&stop_recording_button);
    recording_bar.append(&recording_action_bar);

    let recording_meta_row = GtkBox::new(Orientation::Horizontal, style_tokens.spacing_8);
    recording_meta_row.add_css_class("launchpad-recording-meta");
    recording_meta_row.append(&recording_backend_label);
    recording_meta_row.append(&recording_status_label);

    let recording_source_hint_label =
        Label::new(recording_source_availability.source_hint().as_deref());
    recording_source_hint_label.add_css_class("recording-source-hint");
    recording_source_hint_label.set_halign(Align::End);
    recording_source_hint_label.set_xalign(1.0);
    recording_source_hint_label.set_hexpand(true);
    recording_source_hint_label.set_visible(recording_source_availability.source_hint().is_some());
    recording_meta_row.append(&recording_source_hint_label);

    let recording_content = GtkBox::new(Orientation::Vertical, style_tokens.spacing_8);
    recording_content.append(&recording_bar);
    recording_content.append(&recording_meta_row);
    let recording_panel = launchpad_panel(style_tokens, "Recording", &recording_content);

    // ── Session panel (key-value grid) ──
    let state_label = Label::new(Some("initializing"));
    let status_label = Label::new(Some("Ready"));
    let active_capture_label = Label::new(Some("none"));
    let capture_count_label = Label::new(Some("0"));
    let latest_label = Label::new(Some("No capture yet"));

    let capture_ids_label = Label::new(Some("IDs: none"));
    capture_ids_label.add_css_class("launchpad-capture-ids");
    capture_ids_label.set_halign(Align::Start);
    capture_ids_label.set_xalign(0.0);
    capture_ids_label.set_wrap(true);
    capture_ids_label.set_selectable(true);

    let session_content = GtkBox::new(Orientation::Vertical, style_tokens.spacing_4);
    session_content.append(&launchpad_kv_row("State", &state_label));
    session_content.append(&launchpad_kv_row("Status", &status_label));
    session_content.append(&launchpad_kv_row("Active", &active_capture_label));
    session_content.append(&launchpad_kv_row("Count", &capture_count_label));
    session_content.append(&launchpad_kv_row("Latest", &latest_label));
    session_content.append(&capture_ids_label);
    let session_panel = launchpad_panel(style_tokens, "Session", &session_content);
    session_panel.set_hexpand(true);

    // ── Configuration panel (key-value grid) ──
    let config_content = GtkBox::new(Orientation::Vertical, style_tokens.spacing_4);
    config_content.append(&launchpad_kv_static("Theme", &settings_info.theme_label));
    config_content.append(&launchpad_kv_static(
        "OCR Lang",
        &settings_info.ocr_language_label,
    ));
    config_content.append(&launchpad_kv_static(
        "OCR Models",
        &settings_info.ocr_model_dir_label,
    ));
    config_content.append(&launchpad_kv_static(
        "config.json",
        &settings_info.config_path,
    ));
    config_content.append(&launchpad_kv_static(
        "theme.json",
        &settings_info.theme_config_path,
    ));
    config_content.append(&launchpad_kv_static(
        "keybindings",
        &settings_info.keybinding_config_path,
    ));
    let config_panel = launchpad_panel(style_tokens, "Configuration", &config_content);
    config_panel.set_hexpand(true);

    // ── 2-column info row ──
    let info_row = GtkBox::new(Orientation::Horizontal, style_tokens.spacing_12);
    info_row.add_css_class("launchpad-info-row");
    info_row.append(&session_panel);
    info_row.append(&config_panel);

    // ── Actions panel (unified, 2 rows) ──
    let history_button = Button::with_label("History");
    history_button.set_hexpand(true);
    let open_preview_button = Button::with_label("Open Preview");
    open_preview_button.set_hexpand(true);
    let open_editor_button = Button::with_label("Open Editor");
    open_editor_button.set_hexpand(true);

    let actions_row1 = GtkBox::new(Orientation::Horizontal, style_tokens.spacing_8);
    actions_row1.append(&history_button);
    actions_row1.append(&open_preview_button);
    actions_row1.append(&open_editor_button);

    let save_button = Button::with_label("Save");
    save_button.set_hexpand(true);
    let copy_button = Button::with_label("Copy");
    copy_button.set_hexpand(true);
    let ocr_button = Button::with_label("OCR");
    ocr_button.set_hexpand(true);

    let actions_row2 = GtkBox::new(Orientation::Horizontal, style_tokens.spacing_8);
    actions_row2.append(&save_button);
    actions_row2.append(&copy_button);
    actions_row2.append(&ocr_button);

    let close_preview_button = Button::with_label("Close Preview");
    close_preview_button.set_hexpand(true);
    let close_editor_button = Button::with_label("Close Editor");
    close_editor_button.set_hexpand(true);
    let delete_button = Button::with_label("Delete");
    delete_button.set_hexpand(true);
    delete_button.add_css_class("launchpad-danger-button");

    let actions_row3 = GtkBox::new(Orientation::Horizontal, style_tokens.spacing_8);
    actions_row3.append(&close_preview_button);
    actions_row3.append(&close_editor_button);
    actions_row3.append(&delete_button);

    let actions_content = GtkBox::new(Orientation::Vertical, style_tokens.spacing_8);
    actions_content.append(&actions_row1);
    actions_content.append(&actions_row2);
    actions_content.append(&actions_row3);
    let actions_panel = launchpad_panel(style_tokens, "Actions", &actions_content);

    // ── Scrollable content area ──
    let launchpad_content = GtkBox::new(Orientation::Vertical, style_tokens.spacing_12);
    launchpad_content.append(&capture_panel);
    launchpad_content.append(&recording_panel);
    launchpad_content.append(&info_row);
    launchpad_content.append(&actions_panel);

    let scrolled_window = ScrolledWindow::new();
    scrolled_window.set_policy(gtk4::PolicyType::Never, gtk4::PolicyType::Automatic);
    scrolled_window.set_vexpand(true);
    scrolled_window.set_child(Some(&launchpad_content));

    let hint_label = Label::new(Some(
        "Buttons are enabled only when valid for the current state. (Idle \u{2192} Preview \u{2192} Editor)",
    ));
    hint_label.add_css_class("launchpad-hint");
    hint_label.set_halign(Align::Start);
    hint_label.set_xalign(0.0);
    hint_label.set_wrap(true);

    // ── Assemble root ──
    root.append(&header_row);
    root.append(&subtitle_label);
    root.append(&scrolled_window);
    root.append(&hint_label);
    root.append(&toast_label);

    if !show_launchpad {
        header_row.set_visible(false);
        subtitle_label.set_visible(false);
        scrolled_window.set_visible(false);
        hint_label.set_visible(false);
    }

    LaunchpadUi {
        root,
        toast_label,
        state_label,
        status_label,
        active_capture_label,
        capture_count_label,
        latest_label,
        capture_ids_label,
        full_capture_button,
        region_capture_button,
        window_capture_button,
        record_target_full_button,
        record_target_region_button,
        record_target_window_button,
        record_button,
        pause_recording_button,
        stop_recording_button,
        record_size_combo,
        record_encoding_combo,
        record_system_toggle,
        record_system_combo,
        record_mic_toggle,
        record_mic_combo,
        recording_system_available,
        recording_mic_available,
        recording_backend_label,
        recording_status_label,
        recording_timer_label,
        history_button,
        open_preview_button,
        open_editor_button,
        close_preview_button,
        close_editor_button,
        save_button,
        copy_button,
        ocr_button,
        delete_button,
    }
}

pub(super) fn connect_launchpad_button<F, R>(
    button: &Button,
    launchpad_actions: &LaunchpadActionExecutor,
    render: &Rc<R>,
    action: F,
) where
    F: Fn(&LaunchpadActionExecutor) + 'static,
    R: Fn() + 'static + ?Sized,
{
    let launchpad_actions = launchpad_actions.clone();
    let render = render.clone();
    button.connect_clicked(move |_| {
        action(&launchpad_actions);
        (render.as_ref())();
    });
}

pub(super) fn connect_launchpad_default_buttons<R: Fn() + 'static + ?Sized>(
    launchpad: &LaunchpadUi,
    launchpad_actions: &LaunchpadActionExecutor,
    open_history_window: &Rc<dyn Fn()>,
    start_recording: &Rc<dyn Fn(RecordingRequest)>,
    pause_recording_toggle: &Rc<dyn Fn()>,
    stop_recording: &Rc<dyn Fn()>,
    render: &Rc<R>,
) {
    {
        let launchpad_actions = launchpad_actions.clone();
        let render = render.clone();
        launchpad.full_capture_button.connect_clicked(move |_| {
            let render = render.clone();
            launchpad_actions.capture_and_open_preview_async(
                capture::capture_full,
                "Captured full screen",
                "full capture failed",
                "Full capture failed",
                move || {
                    (render.as_ref())();
                },
            );
        });
    }
    {
        let launchpad_actions = launchpad_actions.clone();
        let render = render.clone();
        launchpad.region_capture_button.connect_clicked(move |_| {
            let render = render.clone();
            launchpad_actions.capture_and_open_preview_async(
                capture::capture_region,
                "Captured selected region",
                "region capture failed",
                "Region capture failed",
                move || {
                    (render.as_ref())();
                },
            );
        });
    }
    {
        let launchpad_actions = launchpad_actions.clone();
        let render = render.clone();
        launchpad.window_capture_button.connect_clicked(move |_| {
            let render = render.clone();
            launchpad_actions.capture_and_open_preview_async(
                capture::capture_window,
                "Captured selected window",
                "window capture failed",
                "Window capture failed",
                move || {
                    (render.as_ref())();
                },
            );
        });
    }
    {
        let launchpad = launchpad.clone();
        let start_recording = start_recording.clone();
        let render = render.clone();
        let button = launchpad.record_button.clone();
        button.connect_clicked(move |_| {
            (start_recording.as_ref())(launchpad.recording_request());
            (render.as_ref())();
        });
    }
    {
        let pause_recording_toggle = pause_recording_toggle.clone();
        let render = render.clone();
        let button = launchpad.pause_recording_button.clone();
        button.connect_clicked(move |_| {
            (pause_recording_toggle.as_ref())();
            (render.as_ref())();
        });
    }
    {
        let stop_recording = stop_recording.clone();
        let render = render.clone();
        launchpad.stop_recording_button.connect_clicked(move |_| {
            (stop_recording.as_ref())();
            (render.as_ref())();
        });
    }
    {
        let open_history_window = open_history_window.clone();
        let render = render.clone();
        launchpad.history_button.connect_clicked(move |_| {
            (open_history_window.as_ref())();
            (render.as_ref())();
        });
    }
    connect_launchpad_button(
        &launchpad.open_preview_button,
        launchpad_actions,
        render,
        |actions| {
            actions.open_preview();
        },
    );
    connect_launchpad_button(
        &launchpad.open_editor_button,
        launchpad_actions,
        render,
        |actions| {
            actions.open_editor();
        },
    );
    connect_launchpad_button(
        &launchpad.close_preview_button,
        launchpad_actions,
        render,
        |actions| {
            actions.close_preview();
        },
    );
    connect_launchpad_button(
        &launchpad.close_editor_button,
        launchpad_actions,
        render,
        |actions| {
            actions.close_editor();
        },
    );
    {
        let launchpad_actions = launchpad_actions.clone();
        let render = render.clone();
        launchpad.save_button.connect_clicked(move |_| {
            let render = render.clone();
            launchpad_actions.run_preview_action_async(PreviewAction::Save, move || {
                (render.as_ref())();
            });
        });
    }
    {
        let launchpad_actions = launchpad_actions.clone();
        let render = render.clone();
        launchpad.copy_button.connect_clicked(move |_| {
            let render = render.clone();
            launchpad_actions.run_preview_action_async(PreviewAction::Copy, move || {
                (render.as_ref())();
            });
        });
    }
    connect_launchpad_button(
        &launchpad.ocr_button,
        launchpad_actions,
        render,
        |actions| {
            actions.run_preview_ocr_action();
        },
    );
    {
        let launchpad_actions = launchpad_actions.clone();
        let render = render.clone();
        launchpad.delete_button.connect_clicked(move |_| {
            let render = render.clone();
            launchpad_actions.delete_active_capture_async(move || {
                (render.as_ref())();
            });
        });
    }
}
