fn main() {
    let startup = agent_screen_ui::StartupConfig {
        capture: agent_screen_ui::StartupCaptureMode::None,
        show_launchpad: false,
        show_history: false,
        daemon_mode: true,
        remote_command: None,
        print_status_json: false,
    };

    if let Err(err) = agent_screen_ui::run_with_config(Some(startup)) {
        eprintln!("Agent Screen daemon failed: {err}");
        std::process::exit(1);
    }
}
