use jjaeng_core::service::RemoteCommand;

#[derive(Debug, Clone, Copy, Default)]
pub enum StartupCaptureMode {
    #[default]
    None,
    Full,
    Region,
    Window,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct StartupConfig {
    pub capture: StartupCaptureMode,
    pub show_launchpad: bool,
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
        let mut config = Self {
            capture: StartupCaptureMode::None,
            show_launchpad: false,
            daemon_mode: false,
            remote_command: None,
            print_status_json: false,
        };

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
                "--launchpad" => {
                    config.show_launchpad = true;
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
                _ => {}
            }
        }

        config
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
}
