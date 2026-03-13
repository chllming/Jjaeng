use gtk4::glib;
use gtk4::prelude::*;
use gtk4::{ComboBoxText, ToggleButton};
use jjaeng_core::recording::{
    self, AudioConfig, AudioMode, AudioSource, RecordingEncodingPreset, RecordingSize,
};

const NO_SYSTEM_AUDIO_SOURCES_LABEL: &str = "No system audio sources";
const NO_MICROPHONE_SOURCES_LABEL: &str = "No microphone sources";

#[derive(Debug, Clone, Default)]
pub(super) struct RecordingSourceAvailability {
    pub(super) system_sources: Vec<AudioSource>,
    pub(super) microphone_sources: Vec<AudioSource>,
    pub(super) system_error: Option<String>,
    pub(super) microphone_error: Option<String>,
}

impl RecordingSourceAvailability {
    pub(super) fn source_hint(&self) -> Option<String> {
        if let Some(message) = self.system_error.as_ref() {
            return Some(format!("System audio unavailable: {message}"));
        }
        if let Some(message) = self.microphone_error.as_ref() {
            return Some(format!("Microphone input unavailable: {message}"));
        }
        if self.system_sources.is_empty() && self.microphone_sources.is_empty() {
            return Some("Audio sources unavailable".to_string());
        }
        None
    }
}

pub(super) fn load_recording_source_availability() -> RecordingSourceAvailability {
    let (system_sources, system_error) = match recording::list_system_audio_sources() {
        Ok(sources) => (sources, None),
        Err(err) => (Vec::new(), Some(err.to_string())),
    };
    let (microphone_sources, microphone_error) = match recording::list_microphone_sources() {
        Ok(sources) => (sources, None),
        Err(err) => (Vec::new(), Some(err.to_string())),
    };

    RecordingSourceAvailability {
        system_sources,
        microphone_sources,
        system_error,
        microphone_error,
    }
}

pub(super) fn populate_system_audio_combo(
    combo: &ComboBoxText,
    sources: &[AudioSource],
    requested_device: Option<&str>,
) {
    populate_source_combo(
        combo,
        sources,
        requested_device,
        recording::default_system_audio_source(),
        NO_SYSTEM_AUDIO_SOURCES_LABEL,
    );
}

pub(super) fn populate_microphone_combo(
    combo: &ComboBoxText,
    sources: &[AudioSource],
    requested_device: Option<&str>,
) {
    populate_source_combo(
        combo,
        sources,
        requested_device,
        recording::default_microphone_source(),
        NO_MICROPHONE_SOURCES_LABEL,
    );
}

pub(super) fn populate_recording_size_combo(combo: &ComboBoxText, size: RecordingSize) {
    combo.append(Some("native"), "Native");
    combo.append(Some("half"), "Half");
    combo.append(Some("fit1080p"), "1080p");
    combo.append(Some("fit720p"), "720p");
    combo.set_active_id(Some(match size {
        RecordingSize::Native => "native",
        RecordingSize::Half => "half",
        RecordingSize::Fit1080p => "fit1080p",
        RecordingSize::Fit720p => "fit720p",
    }));
}

pub(super) fn populate_recording_encoding_combo(
    combo: &ComboBoxText,
    encoding: RecordingEncodingPreset,
) {
    combo.append(Some("standard"), "Standard");
    combo.append(Some("quality"), "High");
    combo.append(Some("small"), "Small");
    combo.set_active_id(Some(match encoding {
        RecordingEncodingPreset::Standard => "standard",
        RecordingEncodingPreset::HighQuality => "quality",
        RecordingEncodingPreset::SmallFile => "small",
    }));
}

pub(super) fn recording_size_from_combo(combo: &ComboBoxText) -> RecordingSize {
    match combo.active_id().as_deref() {
        Some("half") => RecordingSize::Half,
        Some("fit1080p") => RecordingSize::Fit1080p,
        Some("fit720p") => RecordingSize::Fit720p,
        _ => RecordingSize::Native,
    }
}

pub(super) fn recording_encoding_from_combo(combo: &ComboBoxText) -> RecordingEncodingPreset {
    match combo.active_id().as_deref() {
        Some("quality") => RecordingEncodingPreset::HighQuality,
        Some("small") => RecordingEncodingPreset::SmallFile,
        _ => RecordingEncodingPreset::Standard,
    }
}

pub(super) fn apply_audio_config_to_controls(
    system_toggle: &ToggleButton,
    system_combo: &ComboBoxText,
    system_available: bool,
    mic_toggle: &ToggleButton,
    mic_combo: &ComboBoxText,
    microphone_available: bool,
    config: &AudioConfig,
) {
    match config.mode {
        AudioMode::Desktop | AudioMode::Both if system_available => {
            system_toggle.set_active(true);
            mic_toggle.set_active(false);
        }
        AudioMode::Microphone if microphone_available => {
            system_toggle.set_active(false);
            mic_toggle.set_active(true);
        }
        _ => {
            system_toggle.set_active(false);
            mic_toggle.set_active(false);
        }
    }

    sync_audio_controls(
        system_toggle,
        system_combo,
        system_available,
        mic_toggle,
        mic_combo,
        microphone_available,
        true,
    );
}

pub(super) fn connect_audio_toggle_controls(
    system_toggle: &ToggleButton,
    system_combo: &ComboBoxText,
    system_available: bool,
    mic_toggle: &ToggleButton,
    mic_combo: &ComboBoxText,
    microphone_available: bool,
) {
    {
        let system_toggle = system_toggle.clone();
        let system_combo = system_combo.clone();
        let mic_toggle = mic_toggle.clone();
        let mic_combo = mic_combo.clone();
        system_toggle.clone().connect_toggled(move |toggle| {
            if toggle.is_active() && mic_toggle.is_active() {
                mic_toggle.set_active(false);
            }
            sync_audio_controls(
                &system_toggle,
                &system_combo,
                system_available,
                &mic_toggle,
                &mic_combo,
                microphone_available,
                true,
            );
        });
    }
    {
        let system_toggle = system_toggle.clone();
        let system_combo = system_combo.clone();
        let mic_toggle = mic_toggle.clone();
        let mic_combo = mic_combo.clone();
        mic_toggle.clone().connect_toggled(move |toggle| {
            if toggle.is_active() && system_toggle.is_active() {
                system_toggle.set_active(false);
            }
            sync_audio_controls(
                &system_toggle,
                &system_combo,
                system_available,
                &mic_toggle,
                &mic_combo,
                microphone_available,
                true,
            );
        });
    }
}

pub(super) fn sync_audio_controls(
    system_toggle: &ToggleButton,
    system_combo: &ComboBoxText,
    system_available: bool,
    mic_toggle: &ToggleButton,
    mic_combo: &ComboBoxText,
    microphone_available: bool,
    sensitive: bool,
) {
    system_toggle.set_sensitive(sensitive && system_available);
    mic_toggle.set_sensitive(sensitive && microphone_available);
    system_combo.set_sensitive(sensitive && system_available && system_toggle.is_active());
    mic_combo.set_sensitive(sensitive && microphone_available && mic_toggle.is_active());
}

pub(super) fn recording_audio_config_from_controls(
    system_toggle: &ToggleButton,
    system_combo: &ComboBoxText,
    mic_toggle: &ToggleButton,
    mic_combo: &ComboBoxText,
) -> AudioConfig {
    let mut config = AudioConfig::default();
    if mic_toggle.is_active() {
        config.mode = AudioMode::Microphone;
        config.microphone_device = selected_source_value(mic_combo.active_id());
    } else if system_toggle.is_active() {
        config.mode = AudioMode::Desktop;
        config.system_device = selected_source_value(system_combo.active_id());
    }
    config
}

pub(super) fn audio_source_label(source: &AudioSource) -> String {
    source
        .name
        .rsplit_once('.')
        .map(|(_, label)| label.replace('-', " "))
        .unwrap_or_else(|| source.name.clone())
}

fn populate_source_combo(
    combo: &ComboBoxText,
    sources: &[AudioSource],
    requested_device: Option<&str>,
    fallback_device: Option<String>,
    empty_label: &str,
) {
    for source in sources {
        combo.append(Some(&source.name), &audio_source_label(source));
    }

    let selected_device = requested_device
        .map(str::to_string)
        .or(fallback_device)
        .or_else(|| sources.first().map(|source| source.name.clone()));

    if let Some(selected_device) = selected_device {
        if !sources.iter().any(|source| source.name == selected_device) {
            combo.append(Some(&selected_device), &selected_device);
        }
        combo.set_active_id(Some(&selected_device));
    }

    if combo.active_id().is_none() {
        combo.append(Some(""), empty_label);
        combo.set_active_id(Some(""));
    }
}

fn selected_source_value(value: Option<glib::GString>) -> Option<String> {
    value.and_then(|value| {
        let trimmed = value.trim();
        (!trimmed.is_empty()).then(|| trimmed.to_string())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn audio_source_label_uses_last_segment_when_present() {
        let source = AudioSource {
            name: "alsa_input.usb-demo.mono-fallback".to_string(),
        };
        assert_eq!(audio_source_label(&source), "mono fallback");
    }

    #[test]
    fn recording_audio_config_from_controls_prefers_microphone_when_toggled() {
        gtk4::init().ok();

        let system_toggle = ToggleButton::new();
        let system_combo = ComboBoxText::new();
        system_combo.append(Some("system.monitor"), "system");
        system_combo.set_active_id(Some("system.monitor"));

        let mic_toggle = ToggleButton::new();
        let mic_combo = ComboBoxText::new();
        mic_combo.append(Some("mic.source"), "mic");
        mic_combo.set_active_id(Some("mic.source"));

        system_toggle.set_active(true);
        mic_toggle.set_active(true);

        let config = recording_audio_config_from_controls(
            &system_toggle,
            &system_combo,
            &mic_toggle,
            &mic_combo,
        );
        assert_eq!(config.mode, AudioMode::Microphone);
        assert_eq!(config.microphone_device.as_deref(), Some("mic.source"));
    }
}
