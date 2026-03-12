use std::fs;
use std::io::{self, BufRead, BufReader, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::PathBuf;
use std::sync::mpsc::Sender;
use std::thread;

use serde::{Deserialize, Serialize};

use crate::identity::{APP_RUNTIME_SOCKET, APP_STATUS_SNAPSHOT, DEFAULT_RUNTIME_DIR};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum RemoteCommand {
    CaptureFull,
    CaptureRegion,
    CaptureWindow,
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
}

pub fn command_socket_path() -> PathBuf {
    runtime_dir().join(APP_RUNTIME_SOCKET)
}

pub fn status_snapshot_path() -> PathBuf {
    runtime_dir().join(APP_STATUS_SNAPSHOT)
}

pub fn try_send_command(command: &RemoteCommand) -> io::Result<RemoteResponse> {
    let mut stream = UnixStream::connect(command_socket_path())?;
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

pub fn spawn_command_server(sender: Sender<RemoteCommand>) {
    let socket_path = command_socket_path();
    if let Some(parent) = socket_path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    if socket_path.exists() {
        let _ = fs::remove_file(&socket_path);
    }

    let Ok(listener) = UnixListener::bind(&socket_path) else {
        tracing::warn!(path = %socket_path.display(), "failed to bind command socket");
        return;
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
}

pub fn write_status_snapshot(snapshot: &StatusSnapshot) -> io::Result<()> {
    let path = status_snapshot_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    let tmp_path = path.with_extension("json.tmp");
    let encoded = serde_json::to_vec(snapshot).map_err(io::Error::other)?;
    fs::write(&tmp_path, encoded)?;
    fs::rename(tmp_path, path)?;
    Ok(())
}

pub fn read_status_snapshot_json() -> io::Result<String> {
    let path = status_snapshot_path();
    if !path.exists() {
        let snapshot = StatusSnapshot {
            state: "idle".to_string(),
            active_capture_id: None,
            latest_label: "No capture yet".to_string(),
            capture_count: 0,
            preview_count: 0,
            editor_open: false,
        };
        return serde_json::to_string(&snapshot).map_err(io::Error::other);
    }

    fs::read_to_string(path)
}

fn runtime_dir() -> PathBuf {
    std::env::var("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from(DEFAULT_RUNTIME_DIR))
}
