use std::process::Command;

fn jjaeng() -> Command {
    Command::new(env!("CARGO_BIN_EXE_jjaeng"))
}

#[test]
fn cli_short_version_flag() {
    let output = jjaeng().arg("-V").output().unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(output.status.success());
    assert!(stdout.starts_with("Jjaeng "));
    assert!(
        !stdout.contains('('),
        "short version should not contain git hash"
    );
}

#[test]
fn cli_long_version_flag() {
    let output = jjaeng().arg("--version").output().unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(output.status.success());
    assert!(stdout.starts_with("Jjaeng "));
}

#[test]
fn cli_short_help_flag() {
    let output = jjaeng().arg("-h").output().unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(output.status.success());
    assert!(stdout.contains("Usage:"));
    assert!(stdout.contains("--help"));
}

#[test]
fn cli_long_help_flag() {
    let output = jjaeng().arg("--help").output().unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(output.status.success());
    assert!(stdout.contains("Usage:"));
    assert!(stdout.contains("--version"));
}
