fn main() {
    if let Some(code) = handle_early_args() {
        std::process::exit(code);
    }

    if let Err(err) = jjaeng_ui::run() {
        eprintln!("Jjaeng failed: {err}");
        std::process::exit(1);
    }
}

fn handle_early_args() -> Option<i32> {
    let arg = std::env::args().nth(1)?;
    match arg.as_str() {
        "-V" => {
            println!("Jjaeng {}", env!("CARGO_PKG_VERSION"));
            Some(0)
        }
        "--version" => {
            let git_hash = option_env!("GIT_HASH").unwrap_or("");
            if git_hash.is_empty() {
                println!("Jjaeng {}", env!("CARGO_PKG_VERSION"));
            } else {
                println!("Jjaeng {} ({git_hash})", env!("CARGO_PKG_VERSION"));
            }
            Some(0)
        }
        "-h" | "--help" => {
            print_help();
            Some(0)
        }
        "--status-json" => match jjaeng_core::service::read_status_snapshot_json() {
            Ok(json) => {
                println!("{json}");
                Some(0)
            }
            Err(err) => {
                eprintln!("Jjaeng status read failed: {err}");
                Some(1)
            }
        },
        _ => None,
    }
}

fn print_help() {
    println!(
        "\
Jjaeng — Hyprland screenshot preview and editor utility

Usage: jjaeng [OPTIONS]

Options:
  --full, --capture-full        Start with full screen capture
  --region, --capture-region    Start with region capture
  --window, --capture-window    Start with window capture
  --record-full                 Start fullscreen recording
  --record-region               Start region recording
  --record-window               Start window recording
  --record-full-prompt          Select/confirm fullscreen recording with controls
  --record-region-prompt        Select/confirm region recording with controls
  --record-window-prompt        Select/confirm window recording with controls
  --stop-recording              Stop the active recording
  --record-size=<preset>        Recording size: native|half|1080p|720p
  --record-encoding=<preset>    Recording preset: standard|quality|small
  --record-audio=<mode>         Recording audio: off|desktop|mic
  --record-mic=<source>         Pulse/PipeWire source name for microphone mode
  --launchpad                   Show the launchpad
  --history, --open-history     Show the screenshot history window
  --toggle-history              Toggle the screenshot history window
  --daemon                      Run as a hidden GTK daemon with command socket
  --open-preview                Ask a running daemon to show the latest preview
  --edit-latest                 Ask a running daemon to open the latest capture in the editor
  --save-latest                 Ask a running daemon to save the latest capture
  --copy-latest                 Ask a running daemon to copy the latest capture
  --dismiss-latest              Ask a running daemon to dismiss the latest capture
  --status-json                 Print the current daemon status snapshot as JSON
  -V                            Print version
  --version                     Print version (with build info)
  -h, --help                    Print this help message"
    );
}
