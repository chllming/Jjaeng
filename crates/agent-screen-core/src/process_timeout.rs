use std::io;
use std::process::{Child, ExitStatus};
use std::time::{Duration, Instant};

/// Waits for a child process to exit, bounded by `timeout`.
///
/// On timeout the child is killed (SIGKILL) and an `io::ErrorKind::TimedOut` error is returned.
pub fn wait_with_timeout(child: &mut Child, timeout: Duration) -> io::Result<ExitStatus> {
    let id = child.id();
    let deadline = Instant::now() + timeout;
    let poll_interval = Duration::from_millis(50);

    loop {
        match child.try_wait() {
            Ok(Some(status)) => return Ok(status),
            Ok(None) => {}
            Err(e) => return Err(e),
        }

        let now = Instant::now();
        if now >= deadline {
            tracing::warn!(pid = id, "child process timed out, sending SIGKILL");
            let _ = child.kill();
            return child.wait();
        }

        let remaining = deadline - now;
        std::thread::sleep(poll_interval.min(remaining));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;

    #[test]
    fn wait_with_timeout_succeeds_for_fast_process() {
        let mut child = Command::new("true").spawn().expect("spawn true");
        let status = wait_with_timeout(&mut child, Duration::from_secs(5)).expect("wait");
        assert!(status.success());
    }

    #[test]
    fn wait_with_timeout_kills_slow_process() {
        let mut child = Command::new("sleep")
            .arg("60")
            .spawn()
            .expect("spawn sleep");
        let result = wait_with_timeout(&mut child, Duration::from_millis(200));
        assert!(result.is_ok());
        let status = result.unwrap();
        assert!(!status.success());
    }
}
