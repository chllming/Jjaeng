use std::fs;
use std::io::{self, BufRead, BufReader, Write};
use std::os::unix::fs::{FileTypeExt, PermissionsExt};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::PathBuf;
use std::sync::mpsc::Sender;
use std::thread;
use std::time::Duration;

const SOCKET_TIMEOUT: Duration = Duration::from_secs(3);

use serde::{Deserialize, Serialize};

use crate::identity::{
    APP_RUNTIME_SOCKET, APP_STATUS_SNAPSHOT, DEFAULT_RUNTIME_DIR, LEGACY_RUNTIME_DIR,
    UPSTREAM_RUNTIME_DIR,
};
use crate::recording::RecordingRequest;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum RemoteCommand {
    CaptureFull,
    CaptureRegion,
    CaptureWindow,
    StartRecording(RecordingRequest),
    PromptRecording(RecordingRequest),
    StopRecording,
    OpenHistory,
    ToggleHistory,
    OpenPreview,
    OpenEditor,
    SaveLatest,
    CopyLatest,
    DismissLatest,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RemoteResponse {
    pub ok: bool,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct StatusSnapshot {
    pub state: String,
    pub active_capture_id: Option<String>,
    pub latest_label: String,
    pub capture_count: usize,
    pub preview_count: usize,
    pub editor_open: bool,
    pub recording: bool,
    pub recording_duration_ms: Option<u64>,
    pub recording_id: Option<String>,
}

pub fn command_socket_path() -> PathBuf {
    runtime_dir().join(APP_RUNTIME_SOCKET)
}

pub fn status_snapshot_path() -> PathBuf {
    runtime_dir().join(APP_STATUS_SNAPSHOT)
}

pub fn try_send_command(command: &RemoteCommand) -> io::Result<RemoteResponse> {
    let mut stream = connect_command_socket()?;
    stream.set_read_timeout(Some(SOCKET_TIMEOUT))?;
    stream.set_write_timeout(Some(SOCKET_TIMEOUT))?;
    serde_json::to_writer(&mut stream, command)?;
    stream.write_all(b"\n")?;
    stream.flush()?;

    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    reader.read_line(&mut line)?;
    if line.trim().is_empty() {
        return Ok(RemoteResponse {
            ok: true,
            message: "accepted".to_string(),
        });
    }

    serde_json::from_str(line.trim()).map_err(io::Error::other)
}

fn connect_command_socket() -> io::Result<UnixStream> {
    let mut last_error = None;
    for path in [
        command_socket_path(),
        PathBuf::from(LEGACY_RUNTIME_DIR).join("jjaeng.sock"),
        PathBuf::from(UPSTREAM_RUNTIME_DIR).join("chalkak.sock"),
    ] {
        match UnixStream::connect(&path) {
            Ok(stream) => return Ok(stream),
            Err(err) => last_error = Some(err),
        }
    }
    Err(last_error.unwrap_or_else(|| io::Error::new(io::ErrorKind::NotFound, "no command socket")))
}

pub struct CommandServerGuard {
    socket_path: PathBuf,
}

impl Drop for CommandServerGuard {
    fn drop(&mut self) {
        if let Err(err) = fs::remove_file(&self.socket_path) {
            tracing::debug!(
                path = %self.socket_path.display(),
                ?err,
                "failed to clean up command socket on exit"
            );
        }
    }
}

pub fn spawn_command_server(sender: Sender<RemoteCommand>) -> Option<CommandServerGuard> {
    let socket_path = command_socket_path();
    if let Some(parent) = socket_path.parent() {
        if let Err(err) = fs::create_dir_all(parent) {
            tracing::warn!(path = %parent.display(), ?err, "failed to create runtime directory");
            return None;
        }
        if let Err(err) = fs::set_permissions(parent, fs::Permissions::from_mode(0o700)) {
            tracing::warn!(path = %parent.display(), ?err, "failed to harden runtime directory");
            return None;
        }
    }
    if let Ok(metadata) = fs::symlink_metadata(&socket_path) {
        if metadata.file_type().is_symlink() || !metadata.file_type().is_socket() {
            tracing::warn!(path = %socket_path.display(), "refusing to replace non-socket runtime path");
            return None;
        }
        if let Err(err) = fs::remove_file(&socket_path) {
            tracing::warn!(path = %socket_path.display(), ?err, "failed to remove stale command socket");
            return None;
        }
    }

    let Ok(listener) = UnixListener::bind(&socket_path) else {
        tracing::warn!(path = %socket_path.display(), "failed to bind command socket");
        return None;
    };
    if let Err(err) = fs::set_permissions(&socket_path, fs::Permissions::from_mode(0o600)) {
        tracing::warn!(path = %socket_path.display(), ?err, "failed to harden command socket");
        let _ = fs::remove_file(&socket_path);
        return None;
    }

    let guard = CommandServerGuard {
        socket_path: socket_path.clone(),
    };

    thread::spawn(move || {
        for incoming in listener.incoming() {
            let Ok(mut stream) = incoming else {
                continue;
            };

            let mut line = String::new();
            let mut reader = BufReader::new(&mut stream);
            let response = match reader.read_line(&mut line) {
                Ok(0) => RemoteResponse {
                    ok: false,
                    message: "empty command".to_string(),
                },
                Ok(_) => match serde_json::from_str::<RemoteCommand>(line.trim()) {
                    Ok(command) => match sender.send(command) {
                        Ok(()) => RemoteResponse {
                            ok: true,
                            message: "accepted".to_string(),
                        },
                        Err(err) => RemoteResponse {
                            ok: false,
                            message: format!("dispatch failed: {err}"),
                        },
                    },
                    Err(err) => RemoteResponse {
                        ok: false,
                        message: format!("invalid command: {err}"),
                    },
                },
                Err(err) => RemoteResponse {
                    ok: false,
                    message: format!("read failed: {err}"),
                },
            };

            let _ = serde_json::to_writer(&mut stream, &response);
            let _ = stream.write_all(b"\n");
            let _ = stream.flush();
        }
    });

    Some(guard)
}

pub fn write_status_snapshot(snapshot: &StatusSnapshot) -> io::Result<()> {
    let path = status_snapshot_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
        fs::set_permissions(parent, fs::Permissions::from_mode(0o700))?;
    }

    let tmp_path = path.with_extension("json.tmp");
    let encoded = serde_json::to_vec(snapshot).map_err(io::Error::other)?;
    fs::write(&tmp_path, encoded)?;
    fs::rename(tmp_path, path)?;
    fs::set_permissions(status_snapshot_path(), fs::Permissions::from_mode(0o600))?;
    Ok(())
}

pub fn read_status_snapshot_json() -> io::Result<String> {
    let path = [
        status_snapshot_path(),
        PathBuf::from(LEGACY_RUNTIME_DIR).join("jjaeng-status.json"),
        PathBuf::from(UPSTREAM_RUNTIME_DIR).join("chalkak-status.json"),
    ]
    .into_iter()
    .find(|path| path.exists());
    let Some(path) = path else {
        let snapshot = StatusSnapshot {
            state: "idle".to_string(),
            active_capture_id: None,
            latest_label: "No capture yet".to_string(),
            capture_count: 0,
            preview_count: 0,
            editor_open: false,
            recording: false,
            recording_duration_ms: None,
            recording_id: None,
        };
        return serde_json::to_string(&snapshot).map_err(io::Error::other);
    };

    fs::read_to_string(path)
}

fn runtime_dir() -> PathBuf {
    std::env::var("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from(DEFAULT_RUNTIME_DIR))
}
