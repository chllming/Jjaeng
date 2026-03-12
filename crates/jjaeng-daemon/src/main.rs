fn main() {
    let startup = jjaeng_ui::StartupConfig {
        capture: jjaeng_ui::StartupCaptureMode::None,
        show_launchpad: false,
        show_history: false,
        daemon_mode: true,
        remote_command: None,
        print_status_json: false,
    };

    if let Err(err) = jjaeng_ui::run_with_config(Some(startup)) {
        eprintln!("Jjaeng daemon failed: {err}");
        std::process::exit(1);
    }
}
