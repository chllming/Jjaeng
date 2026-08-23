use std::io::{self, Read as _, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;

use thiserror::Error;

const WL_COPY_TIMEOUT: Duration = Duration::from_secs(5);

const MIME_TEXT_PLAIN_UTF8: &str = "text/plain;charset=utf-8";
const MIME_IMAGE_PNG: &str = "image/png";

#[derive(Debug, Error)]
pub enum ClipboardError {
    #[error("failed to run wl-copy command: {command}")]
    CommandIo {
        command: String,
        #[source]
        source: io::Error,
    },
    #[error("failed to read image file {path}: {source}")]
    ReadFile {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
}

pub type ClipboardResult<T> = std::result::Result<T, ClipboardError>;

pub trait ClipboardBackend {
    fn copy(&self, path: &Path) -> ClipboardResult<()>;
}

#[derive(Debug, Default)]
pub struct WlCopyBackend;

fn is_png_path(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| ext.eq_ignore_ascii_case("png"))
}

fn resolve_absolute_path(path: &Path) -> ClipboardResult<PathBuf> {
    if path.is_absolute() {
        return Ok(path.to_path_buf());
    }
    std::env::current_dir()
        .map(|dir| dir.join(path))
        .map_err(|err| ClipboardError::CommandIo {
            command: "current_dir".to_string(),
            source: err,
        })
}

#[derive(Debug, Eq, PartialEq)]
struct ClipboardPayload {
    mime_type: &'static str,
    bytes: Vec<u8>,
}

fn clipboard_payload(path: &Path) -> ClipboardResult<ClipboardPayload> {
    let absolute_path = resolve_absolute_path(path)?;
    if is_png_path(&absolute_path) {
        let image_bytes =
            std::fs::read(&absolute_path).map_err(|source| ClipboardError::ReadFile {
                path: absolute_path,
                source,
            })?;
        return Ok(ClipboardPayload {
            mime_type: MIME_IMAGE_PNG,
            bytes: image_bytes,
        });
    }

    Ok(ClipboardPayload {
        mime_type: MIME_TEXT_PLAIN_UTF8,
        bytes: absolute_path.to_string_lossy().into_owned().into_bytes(),
    })
}

fn copy_with_wl_copy(payload: &ClipboardPayload) -> ClipboardResult<()> {
    let command = format!("wl-copy --type {}", payload.mime_type);
    let mut child = Command::new("wl-copy")
        .arg("--type")
        .arg(payload.mime_type)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|source| ClipboardError::CommandIo {
            command: command.clone(),
            source,
        })?;

    if let Some(mut stdin) = child.stdin.take() {
        stdin
            .write_all(&payload.bytes)
            .map_err(|source| ClipboardError::CommandIo {
                command: command.clone(),
                source,
            })?;
    }

    let status = crate::process_timeout::wait_with_timeout(&mut child, WL_COPY_TIMEOUT).map_err(
        |source| ClipboardError::CommandIo {
            command: command.clone(),
            source,
        },
    )?;

    if status.success() {
        return Ok(());
    }

    let mut stderr_content = String::new();
    if let Some(mut stderr) = child.stderr.take() {
        let _ = stderr.read_to_string(&mut stderr_content);
    }
    let stderr_trimmed = stderr_content.trim();
    let message = if stderr_trimmed.is_empty() {
        format!("exit status {status}")
    } else {
        format!("exit status {status}: {stderr_trimmed}")
    };
    Err(ClipboardError::CommandIo {
        command,
        source: io::Error::other(message),
    })
}

pub fn clipboard_available() -> bool {
    Command::new("wl-copy")
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok()
}

impl ClipboardBackend for WlCopyBackend {
    fn copy(&self, path: &Path) -> ClipboardResult<()> {
        let payload = clipboard_payload(path)?;
        copy_with_wl_copy(&payload)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;

    struct DummyBackend;
    impl ClipboardBackend for DummyBackend {
        fn copy(&self, _path: &Path) -> ClipboardResult<()> {
            Ok(())
        }
    }

    #[test]
    fn copy_success_with_backend() {
        let temp_dir = env::temp_dir();
        let file_path = temp_dir.join("jjaeng-copy-ref-test.png");
        std::fs::write(&file_path, b"binary").unwrap();
        let result = DummyBackend.copy(&file_path);
        assert!(result.is_ok());
        let _ = std::fs::remove_file(file_path);
    }

    #[test]
    fn clipboard_payload_reads_png_bytes_for_images() {
        let temp_dir = env::temp_dir();
        let file_path = temp_dir.join("jjaeng clipboard payload image test.png");
        std::fs::write(&file_path, b"image-binary").unwrap();
        let payload = clipboard_payload(&file_path).expect("payload");
        assert_eq!(
            payload,
            ClipboardPayload {
                mime_type: MIME_IMAGE_PNG,
                bytes: b"image-binary".to_vec(),
            }
        );
        let _ = std::fs::remove_file(file_path);
    }

    #[test]
    fn clipboard_payload_uses_absolute_text_path_for_non_images() {
        let temp_dir = env::temp_dir();
        let file_path = temp_dir.join("jjaeng clipboard payload text test.mp4");
        std::fs::write(&file_path, b"binary").unwrap();
        let payload = clipboard_payload(&file_path).expect("payload");
        assert_eq!(payload.mime_type, MIME_TEXT_PLAIN_UTF8);
        assert_eq!(payload.bytes, file_path.to_string_lossy().as_bytes());
        let _ = std::fs::remove_file(file_path);
    }

    #[test]
    fn is_png_path_detects_case_insensitive_png_extension() {
        assert!(is_png_path(Path::new("/tmp/capture.PNG")));
        assert!(is_png_path(Path::new("/tmp/capture.png")));
        assert!(!is_png_path(Path::new("/tmp/capture.jpg")));
    }
}
