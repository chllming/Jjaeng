use jjaeng_core::config::{load_app_config, AppConfig};
use jjaeng_core::recording::{
    AudioMode, RecordingEncodingPreset, RecordingRequest, RecordingSize, RecordingTarget,
};
use jjaeng_core::service::RemoteCommand;

#[derive(Debug, Clone, Copy, Default)]
pub enum StartupCaptureMode {
    #[default]
    None,
    Full,
    Region,
    Window,
}

#[derive(Debug, Clone, Default)]
pub struct StartupConfig {
    pub capture: StartupCaptureMode,
    pub show_launchpad: bool,
    pub show_history: bool,
    pub daemon_mode: bool,
    pub remote_command: Option<RemoteCommand>,
    pub print_status_json: bool,
}

impl StartupConfig {
    pub(crate) fn from_args() -> Self {
        Self::from_iter(std::env::args().skip(1))
    }

    fn from_iter<I, S>(args: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let app_config = load_app_config();
        let mut config = Self {
            capture: StartupCaptureMode::None,
            show_launchpad: false,
            show_history: false,
            daemon_mode: false,
            remote_command: None,
            print_status_json: false,
        };
        let mut recording_request: Option<RecordingRequest> = None;
        let mut recording_prompt = false;

        for arg in args {
            match arg.as_ref() {
                "--capture-full" | "--full" => {
                    config.capture = StartupCaptureMode::Full;
                    config.remote_command = Some(RemoteCommand::CaptureFull);
                }
                "--capture-region" | "--region" => {
                    config.capture = StartupCaptureMode::Region;
                    config.remote_command = Some(RemoteCommand::CaptureRegion);
                }
                "--capture-window" | "--window" => {
                    config.capture = StartupCaptureMode::Window;
                    config.remote_command = Some(RemoteCommand::CaptureWindow);
                }
                "--record-full" => {
                    recording_request = Some(default_recording_request(
                        &app_config,
                        RecordingTarget::Fullscreen,
                    ));
                    recording_prompt = false;
                }
                "--record-region" => {
                    recording_request = Some(default_recording_request(
                        &app_config,
                        RecordingTarget::Region,
                    ));
                    recording_prompt = false;
                }
                "--record-window" => {
                    recording_request = Some(default_recording_request(
                        &app_config,
                        RecordingTarget::Window,
                    ));
                    recording_prompt = false;
                }
                "--record-full-prompt" => {
                    recording_request = Some(default_recording_request(
                        &app_config,
                        RecordingTarget::Fullscreen,
                    ));
                    recording_prompt = true;
                }
                "--record-region-prompt" => {
                    recording_request = Some(default_recording_request(
                        &app_config,
                        RecordingTarget::Region,
                    ));
                    recording_prompt = true;
                }
                "--record-window-prompt" => {
                    recording_request = Some(default_recording_request(
                        &app_config,
                        RecordingTarget::Window,
                    ));
                    recording_prompt = true;
                }
                "--stop-recording" => {
                    recording_request = None;
                    recording_prompt = false;
                    config.remote_command = Some(RemoteCommand::StopRecording);
                }
                "--launchpad" => {
                    config.show_launchpad = true;
                }
                "--history" | "--open-history" => {
                    config.show_history = true;
                    config.remote_command = Some(RemoteCommand::OpenHistory);
                }
                "--toggle-history" => {
                    config.remote_command = Some(RemoteCommand::ToggleHistory);
                }
                "--daemon" => {
                    config.daemon_mode = true;
                }
                "--open-preview" => {
                    config.remote_command = Some(RemoteCommand::OpenPreview);
                }
                "--edit-latest" => {
                    config.remote_command = Some(RemoteCommand::OpenEditor);
                }
                "--save-latest" => {
                    config.remote_command = Some(RemoteCommand::SaveLatest);
                }
                "--copy-latest" => {
                    config.remote_command = Some(RemoteCommand::CopyLatest);
                }
                "--dismiss-latest" => {
                    config.remote_command = Some(RemoteCommand::DismissLatest);
                }
                "--status-json" => {
                    config.print_status_json = true;
                }
                _ if arg.as_ref().starts_with("--record-size=") => {
                    if let Some(request) = recording_request.as_mut() {
                        if let Some(size) = parse_recording_size(arg.as_ref()) {
                            request.options.size = size;
                        }
                    }
                }
                _ if arg.as_ref().starts_with("--record-encoding=") => {
                    if let Some(request) = recording_request.as_mut() {
                        if let Some(encoding) = parse_recording_encoding(arg.as_ref()) {
                            request.options.encoding = encoding;
                        }
                    }
                }
                _ if arg.as_ref().starts_with("--record-audio=") => {
                    if let Some(request) = recording_request.as_mut() {
                        if let Some(audio_mode) = parse_recording_audio(arg.as_ref()) {
                            request.options.audio.mode = audio_mode;
                        }
                    }
                }
                _ if arg.as_ref().starts_with("--record-system=") => {
                    if let Some(request) = recording_request.as_mut() {
                        request.options.audio.system_device =
                            parse_key_value(arg.as_ref()).map(str::to_string);
                    }
                }
                _ if arg.as_ref().starts_with("--record-mic=") => {
                    if let Some(request) = recording_request.as_mut() {
                        request.options.audio.microphone_device =
                            parse_key_value(arg.as_ref()).map(str::to_string);
                    }
                }
                _ => {}
            }
        }

        if let Some(request) = recording_request {
            config.remote_command = Some(if recording_prompt {
                RemoteCommand::PromptRecording(request)
            } else {
                RemoteCommand::StartRecording(request)
            });
        }

        config
    }
}

fn default_recording_request(app_config: &AppConfig, target: RecordingTarget) -> RecordingRequest {
    let mut request = RecordingRequest::new(target);
    if let Some(size) = app_config.recording_size {
        request.options.size = size;
    }
    if let Some(encoding) = app_config.recording_encoding_preset {
        request.options.encoding = encoding;
    }
    if let Some(audio_mode) = app_config.recording_audio_mode {
        request.options.audio.mode = audio_mode;
    }
    request.options.audio.system_device = app_config.recording_system_device.clone();
    request.options.audio.microphone_device = app_config.recording_mic_device.clone();
    request
}

fn parse_key_value(arg: &str) -> Option<&str> {
    arg.split_once('=')
        .map(|(_, value)| value.trim())
        .filter(|value| !value.is_empty())
}

fn parse_recording_size(arg: &str) -> Option<RecordingSize> {
    match parse_key_value(arg)? {
        "native" => Some(RecordingSize::Native),
        "half" => Some(RecordingSize::Half),
        "1080p" | "fit1080p" => Some(RecordingSize::Fit1080p),
        "720p" | "fit720p" => Some(RecordingSize::Fit720p),
        _ => None,
    }
}

fn parse_recording_encoding(arg: &str) -> Option<RecordingEncodingPreset> {
    match parse_key_value(arg)? {
        "standard" => Some(RecordingEncodingPreset::Standard),
        "quality" | "high" | "high-quality" => Some(RecordingEncodingPreset::HighQuality),
        "small" | "small-file" => Some(RecordingEncodingPreset::SmallFile),
        _ => None,
    }
}

fn parse_recording_audio(arg: &str) -> Option<AudioMode> {
    match parse_key_value(arg)? {
        "off" | "none" => Some(AudioMode::Off),
        "desktop" => Some(AudioMode::Desktop),
        "mic" | "microphone" => Some(AudioMode::Microphone),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn startup_config_parses_capture_modes() {
        let full = StartupConfig::from_iter(["--full"]);
        assert!(matches!(full.capture, StartupCaptureMode::Full));

        let region = StartupConfig::from_iter(["--capture-region"]);
        assert!(matches!(region.capture, StartupCaptureMode::Region));

        let window = StartupConfig::from_iter(["--capture-window"]);
        assert!(matches!(window.capture, StartupCaptureMode::Window));
    }

    #[test]
    fn startup_config_enables_launchpad_flag() {
        let config = StartupConfig::from_iter(["--launchpad"]);
        assert!(config.show_launchpad);
    }

    #[test]
    fn startup_config_enables_history_flag() {
        let config = StartupConfig::from_iter(["--history"]);
        assert!(config.show_history);
        assert_eq!(config.remote_command, Some(RemoteCommand::OpenHistory));
    }

    #[test]
    fn startup_config_parses_toggle_history_flag() {
        let config = StartupConfig::from_iter(["--toggle-history"]);
        assert!(!config.show_history);
        assert_eq!(config.remote_command, Some(RemoteCommand::ToggleHistory));
    }

    #[test]
    fn startup_config_parses_daemon_and_status_flags() {
        let config = StartupConfig::from_iter(["--daemon", "--status-json"]);
        assert!(config.daemon_mode);
        assert!(config.print_status_json);
    }

    #[test]
    fn startup_config_parses_remote_commands() {
        let config = StartupConfig::from_iter(["--copy-latest"]);
        assert_eq!(config.remote_command, Some(RemoteCommand::CopyLatest));
    }

    #[test]
    fn startup_config_last_capture_flag_wins() {
        let config = StartupConfig::from_iter(["--full", "--region", "--window"]);
        assert!(matches!(config.capture, StartupCaptureMode::Window));
    }

    #[test]
    fn startup_config_parses_recording_request() {
        let config = StartupConfig::from_iter([
            "--record-region",
            "--record-size=half",
            "--record-encoding=small",
            "--record-audio=desktop",
        ]);
        let Some(RemoteCommand::StartRecording(request)) = config.remote_command else {
            panic!("expected recording request");
        };
        assert_eq!(request.target, RecordingTarget::Region);
        assert_eq!(request.options.size, RecordingSize::Half);
        assert_eq!(request.options.encoding, RecordingEncodingPreset::SmallFile);
        assert_eq!(request.options.audio.mode, AudioMode::Desktop);
    }

    #[test]
    fn startup_config_parses_recording_prompt_request() {
        let config = StartupConfig::from_iter(["--record-region-prompt"]);
        let Some(RemoteCommand::PromptRecording(request)) = config.remote_command else {
            panic!("expected recording prompt request");
        };
        assert_eq!(request.target, RecordingTarget::Region);
    }

    #[test]
    fn startup_config_parses_stop_recording_flag() {
        let config = StartupConfig::from_iter(["--stop-recording"]);
        assert_eq!(config.remote_command, Some(RemoteCommand::StopRecording));
    }

    #[test]
    fn parse_recording_audio_rejects_unsupported_audio_mix() {
        assert_eq!(parse_recording_audio("--record-audio=both"), None);
    }
}
