use std::process::Command;

fn agent_screen() -> Command {
    Command::new(env!("CARGO_BIN_EXE_jjaeng"))
}

#[test]
fn cli_short_version_flag() {
    let output = agent_screen().arg("-V").output().unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(output.status.success());
    assert!(stdout.starts_with("Agent Screen "));
    assert!(
        !stdout.contains('('),
        "short version should not contain git hash"
    );
}

#[test]
fn cli_long_version_flag() {
    let output = agent_screen().arg("--version").output().unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(output.status.success());
    assert!(stdout.starts_with("Agent Screen "));
}

#[test]
fn cli_short_help_flag() {
    let output = agent_screen().arg("-h").output().unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(output.status.success());
    assert!(stdout.contains("Usage:"));
    assert!(stdout.contains("--help"));
}

#[test]
fn cli_long_help_flag() {
    let output = agent_screen().arg("--help").output().unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(output.status.success());
    assert!(stdout.contains("Usage:"));
    assert!(stdout.contains("--version"));
}
