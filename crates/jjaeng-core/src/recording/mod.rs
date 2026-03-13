use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};

use image::{Rgba, RgbaImage};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::capture;
use crate::storage::create_temp_recording;

const DEFAULT_RECORDING_EXTENSION: &str = "mp4";
const RECORDING_BACKEND_COMMAND: &str = "wl-screenrec";
const RECORDING_THUMBNAIL_WIDTH: u32 = 320;
const RECORDING_THUMBNAIL_HEIGHT: u32 = 180;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum RecordingTarget {
    #[default]
    Fullscreen,
    Region,
    Window,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum RecordingSize {
    #[default]
    Native,
    Half,
    Fit1080p,
    Fit720p,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum RecordingEncodingPreset {
    #[default]
    Standard,
    HighQuality,
    SmallFile,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum AudioMode {
    #[default]
    Off,
    Desktop,
    Microphone,
    Both,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct AudioConfig {
    #[serde(default)]
    pub mode: AudioMode,
    #[serde(default)]
    pub microphone_device: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AudioSource {
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct RecordingAdvancedOverrides {
    #[serde(default)]
    pub container: Option<String>,
    #[serde(default)]
    pub video_codec: Option<String>,
    #[serde(default)]
    pub video_bitrate: Option<String>,
    #[serde(default)]
    pub audio_codec: Option<String>,
    #[serde(default)]
    pub audio_bitrate: Option<String>,
    #[serde(default)]
    pub fps: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RecordingOptions {
    pub size: RecordingSize,
    pub encoding: RecordingEncodingPreset,
    #[serde(default)]
    pub audio: AudioConfig,
    #[serde(default)]
    pub advanced: Option<RecordingAdvancedOverrides>,
}

impl Default for RecordingOptions {
    fn default() -> Self {
        Self {
            size: RecordingSize::Native,
            encoding: RecordingEncodingPreset::Standard,
            audio: AudioConfig::default(),
            advanced: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RecordingRequest {
    pub target: RecordingTarget,
    #[serde(default)]
    pub options: RecordingOptions,
}

impl RecordingRequest {
    pub fn new(target: RecordingTarget) -> Self {
        Self {
            target,
            options: RecordingOptions::default(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedRecordingOptions {
    pub container_extension: String,
    pub encode_resolution: Option<String>,
    pub video_codec: Option<String>,
    pub video_bitrate: Option<String>,
    pub audio_codec: Option<String>,
    pub audio_bitrate: Option<String>,
    pub max_fps: Option<u32>,
    pub audio_enabled: bool,
    pub audio_device: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RecordGeometry {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RecordingSelection {
    Fullscreen {
        monitor_name: String,
        geometry: RecordGeometry,
    },
    Region {
        geometry_string: String,
        geometry: RecordGeometry,
    },
    Window {
        geometry_string: String,
        geometry: RecordGeometry,
    },
}

impl RecordingSelection {
    pub fn target(&self) -> RecordingTarget {
        match self {
            Self::Fullscreen { .. } => RecordingTarget::Fullscreen,
            Self::Region { .. } => RecordingTarget::Region,
            Self::Window { .. } => RecordingTarget::Window,
        }
    }

    pub fn geometry(&self) -> RecordGeometry {
        match self {
            Self::Fullscreen { geometry, .. }
            | Self::Region { geometry, .. }
            | Self::Window { geometry, .. } => *geometry,
        }
    }
}

#[derive(Debug)]
pub struct RecordingHandle {
    pub child: Child,
    pub recording_id: String,
    pub output_path: PathBuf,
    pub started_at: u64,
    pub geometry: RecordGeometry,
    pub options: RecordingOptions,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordArtifact {
    pub recording_id: String,
    pub output_path: PathBuf,
    pub thumbnail_path: PathBuf,
    pub width: u32,
    pub height: u32,
    pub duration_ms: u64,
    pub file_size_bytes: u64,
    pub created_at: u64,
    pub audio_config: AudioConfig,
}

#[derive(Debug, Error)]
pub enum RecordError {
    #[error("capture selection failed: {0}")]
    CaptureSelection(#[from] crate::capture::CaptureError),
    #[error("failed to spawn recorder: {command}")]
    SpawnFailed {
        command: String,
        #[source]
        source: std::io::Error,
    },
    #[error("recorder command failed: {command}")]
    CommandFailed { command: String, message: String },
    #[error("failed to stop recorder process")]
    StopFailed {
        #[source]
        source: std::io::Error,
    },
    #[error("failed to send recorder signal {signal}")]
    SignalFailed {
        signal: String,
        #[source]
        source: std::io::Error,
    },
    #[error("recording output missing: {0}")]
    OutputMissing(#[from] std::io::Error),
    #[error("invalid geometry: {0}")]
    InvalidGeometry(String),
    #[error("unsupported audio mode: {0}")]
    UnsupportedAudioMode(String),
    #[error("missing recording dependency: {0}")]
    MissingDependency(String),
    #[error("failed to parse media metadata: {0}")]
    Metadata(String),
}

pub trait RecordBackend {
    fn start_fullscreen(
        &self,
        monitor: &str,
        output: &Path,
        options: &ResolvedRecordingOptions,
    ) -> Result<Child, RecordError>;

    fn start_region(
        &self,
        geometry: &str,
        output: &Path,
        options: &ResolvedRecordingOptions,
    ) -> Result<Child, RecordError>;

    fn start_window(
        &self,
        geometry: &str,
        output: &Path,
        options: &ResolvedRecordingOptions,
    ) -> Result<Child, RecordError>;
}

#[derive(Debug, Default)]
pub struct SystemRecordBackend;

impl RecordBackend for SystemRecordBackend {
    fn start_fullscreen(
        &self,
        monitor: &str,
        output: &Path,
        options: &ResolvedRecordingOptions,
    ) -> Result<Child, RecordError> {
        let mut command = base_record_command(output, options);
        command.arg("-o").arg(monitor);
        spawn_record_command(command)
    }

    fn start_region(
        &self,
        geometry: &str,
        output: &Path,
        options: &ResolvedRecordingOptions,
    ) -> Result<Child, RecordError> {
        let mut command = base_record_command(output, options);
        command.arg("-g").arg(geometry);
        spawn_record_command(command)
    }

    fn start_window(
        &self,
        geometry: &str,
        output: &Path,
        options: &ResolvedRecordingOptions,
    ) -> Result<Child, RecordError> {
        self.start_region(geometry, output, options)
    }
}

pub fn recording_backend_available() -> bool {
    command_available(RECORDING_BACKEND_COMMAND)
}

pub fn recording_backend_requirement_message() -> String {
    format!("recording requires `{RECORDING_BACKEND_COMMAND}` to be installed")
}

pub fn start_recording(request: &RecordingRequest) -> Result<RecordingHandle, RecordError> {
    start_recording_with(&SystemRecordBackend, request)
}

pub fn start_recording_selected(
    request: &RecordingRequest,
    selection: &RecordingSelection,
) -> Result<RecordingHandle, RecordError> {
    start_recording_with_selection(&SystemRecordBackend, request, selection)
}

pub fn stop_recording(handle: &mut RecordingHandle) -> Result<RecordArtifact, RecordError> {
    stop_recording_with(handle)
}

pub fn pause_recording(handle: &RecordingHandle) -> Result<(), RecordError> {
    send_recording_signal(handle, "-STOP")
}

pub fn resume_recording(handle: &RecordingHandle) -> Result<(), RecordError> {
    send_recording_signal(handle, "-CONT")
}

pub fn list_microphone_sources() -> Result<Vec<AudioSource>, RecordError> {
    let output = Command::new("pactl")
        .args(["list", "short", "sources"])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()
        .map_err(|source| RecordError::SpawnFailed {
            command: "pactl".to_string(),
            source,
        })?;
    if !output.status.success() {
        return Err(RecordError::CommandFailed {
            command: "pactl".to_string(),
            message: format!("exit status {:?}", output.status.code()),
        });
    }

    Ok(String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(parse_pactl_source)
        .filter(|source| !source.name.contains(".monitor"))
        .collect())
}

pub fn default_microphone_source() -> Option<String> {
    let output = Command::new("pactl")
        .arg("get-default-source")
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }

    let source = String::from_utf8_lossy(&output.stdout).trim().to_string();
    (!source.is_empty() && !source.contains(".monitor")).then_some(source)
}

pub fn resolve_recording_selection(
    target: RecordingTarget,
) -> Result<RecordingSelection, RecordError> {
    match target {
        RecordingTarget::Fullscreen => {
            let monitor = capture::focused_monitor_target()?;
            Ok(RecordingSelection::Fullscreen {
                monitor_name: monitor.name,
                geometry: RecordGeometry {
                    x: monitor.x,
                    y: monitor.y,
                    width: monitor.width,
                    height: monitor.height,
                },
            })
        }
        RecordingTarget::Region => {
            let geometry_string = capture::select_region_geometry()?;
            let geometry = parse_geometry(&geometry_string)?;
            Ok(RecordingSelection::Region {
                geometry_string,
                geometry,
            })
        }
        RecordingTarget::Window => {
            let geometry_string = capture::select_window_geometry()?;
            let geometry = parse_geometry(&geometry_string)?;
            Ok(RecordingSelection::Window {
                geometry_string,
                geometry,
            })
        }
    }
}

pub fn start_recording_with<B: RecordBackend>(
    backend: &B,
    request: &RecordingRequest,
) -> Result<RecordingHandle, RecordError> {
    let selection = resolve_recording_selection(request.target)?;
    start_recording_with_selection(backend, request, &selection)
}

pub fn start_recording_with_selection<B: RecordBackend>(
    backend: &B,
    request: &RecordingRequest,
    selection: &RecordingSelection,
) -> Result<RecordingHandle, RecordError> {
    if !recording_backend_available() {
        return Err(RecordError::MissingDependency(
            RECORDING_BACKEND_COMMAND.to_string(),
        ));
    }
    let started_at = now_millis()?;
    let recording_id = format!("recording-{}", started_at.saturating_mul(1_000_000));

    match selection {
        RecordingSelection::Fullscreen {
            monitor_name,
            geometry,
        } => {
            let resolved_options =
                resolve_recording_options(&request.options, geometry.width, geometry.height)?;
            let output_path =
                create_temp_recording(&recording_id, &resolved_options.container_extension);
            let child =
                backend.start_fullscreen(monitor_name.as_str(), &output_path, &resolved_options)?;
            Ok(RecordingHandle {
                child,
                recording_id,
                output_path,
                started_at,
                geometry: *geometry,
                options: request.options.clone(),
            })
        }
        RecordingSelection::Region {
            geometry_string,
            geometry,
        }
        | RecordingSelection::Window {
            geometry_string,
            geometry,
        } => {
            let resolved_options =
                resolve_recording_options(&request.options, geometry.width, geometry.height)?;
            let output_path =
                create_temp_recording(&recording_id, &resolved_options.container_extension);
            let child = match selection {
                RecordingSelection::Region { .. } => {
                    backend.start_region(geometry_string, &output_path, &resolved_options)?
                }
                RecordingSelection::Window { .. } => {
                    backend.start_window(geometry_string, &output_path, &resolved_options)?
                }
                RecordingSelection::Fullscreen { .. } => unreachable!(),
            };
            Ok(RecordingHandle {
                child,
                recording_id,
                output_path,
                started_at,
                geometry: *geometry,
                options: request.options.clone(),
            })
        }
    }
}

pub fn stop_recording_with(handle: &mut RecordingHandle) -> Result<RecordArtifact, RecordError> {
    let pid = handle.child.id().to_string();
    let stop = Command::new("kill")
        .args(["-INT", pid.as_str()])
        .status()
        .map_err(|source| RecordError::StopFailed { source })?;
    if !stop.success() {
        return Err(RecordError::CommandFailed {
            command: "kill -INT".to_string(),
            message: format!("exit status {:?}", stop.code()),
        });
    }

    let status = handle.child.wait().map_err(RecordError::OutputMissing)?;
    if !status.success() {
        tracing::warn!(
            recording_id = %handle.recording_id,
            status = ?status.code(),
            "recording process exited non-zero"
        );
    }

    let metadata = fs::metadata(&handle.output_path).map_err(RecordError::OutputMissing)?;
    let probe = probe_video_metadata(&handle.output_path).unwrap_or(VideoMetadata {
        width: handle.geometry.width,
        height: handle.geometry.height,
        duration_ms: now_millis()?.saturating_sub(handle.started_at),
        file_size_bytes: metadata.len(),
    });
    let thumbnail_path = handle.output_path.with_extension("thumb.png");
    if let Err(err) = extract_thumbnail(&handle.output_path, &thumbnail_path) {
        tracing::warn!(
            recording_id = %handle.recording_id,
            ?err,
            "failed to extract recording thumbnail; writing placeholder"
        );
        write_recording_placeholder_thumbnail(&thumbnail_path)?;
    }

    Ok(RecordArtifact {
        recording_id: handle.recording_id.clone(),
        output_path: handle.output_path.clone(),
        thumbnail_path,
        width: probe.width,
        height: probe.height,
        duration_ms: probe.duration_ms,
        file_size_bytes: probe.file_size_bytes,
        created_at: handle.started_at,
        audio_config: handle.options.audio.clone(),
    })
}

fn base_record_command(output: &Path, options: &ResolvedRecordingOptions) -> Command {
    let mut command = Command::new(RECORDING_BACKEND_COMMAND);
    command.arg("-f").arg(output);
    if let Some(size) = options.encode_resolution.as_ref() {
        command.arg("--encode-resolution").arg(size);
    }
    if let Some(codec) = options.video_codec.as_ref() {
        command.arg("--codec").arg(codec);
    }
    if let Some(bitrate) = options.video_bitrate.as_ref() {
        command.arg("--bitrate").arg(bitrate);
    }
    if let Some(fps) = options.max_fps {
        command.arg("--max-fps").arg(fps.to_string());
    }
    if options.audio_enabled {
        command.arg("--audio");
    }
    if let Some(device) = options.audio_device.as_ref() {
        command.arg("--audio-device").arg(device);
    }
    if let Some(codec) = options.audio_codec.as_ref() {
        command.arg("--audio-codec").arg(codec);
    }
    if let Some(bitrate) = options.audio_bitrate.as_ref() {
        command.arg("--audio-bitrate").arg(bitrate);
    }
    command.stdin(Stdio::null());
    command.stdout(Stdio::null());
    command.stderr(Stdio::null());
    command
}

fn send_recording_signal(handle: &RecordingHandle, signal: &str) -> Result<(), RecordError> {
    let pid = handle.child.id().to_string();
    let status = Command::new("kill")
        .args([signal, pid.as_str()])
        .status()
        .map_err(|source| RecordError::SignalFailed {
            signal: signal.to_string(),
            source,
        })?;
    if status.success() {
        Ok(())
    } else {
        Err(RecordError::CommandFailed {
            command: format!("kill {signal}"),
            message: format!("exit status {:?}", status.code()),
        })
    }
}

fn spawn_record_command(mut command: Command) -> Result<Child, RecordError> {
    command.spawn().map_err(|source| RecordError::SpawnFailed {
        command: format!("{command:?}"),
        source,
    })
}

fn resolve_recording_options(
    options: &RecordingOptions,
    source_width: u32,
    source_height: u32,
) -> Result<ResolvedRecordingOptions, RecordError> {
    let mut resolved = match options.encoding {
        RecordingEncodingPreset::Standard => ResolvedRecordingOptions {
            container_extension: DEFAULT_RECORDING_EXTENSION.to_string(),
            encode_resolution: scale_resolution(options.size, source_width, source_height)
                .map(|(width, height)| format!("{width}x{height}")),
            video_codec: Some("h264".to_string()),
            video_bitrate: Some("12M".to_string()),
            audio_codec: Some("aac".to_string()),
            audio_bitrate: Some("160k".to_string()),
            max_fps: Some(60),
            audio_enabled: !matches!(options.audio.mode, AudioMode::Off),
            audio_device: None,
        },
        RecordingEncodingPreset::HighQuality => ResolvedRecordingOptions {
            container_extension: DEFAULT_RECORDING_EXTENSION.to_string(),
            encode_resolution: scale_resolution(options.size, source_width, source_height)
                .map(|(width, height)| format!("{width}x{height}")),
            video_codec: Some("h264".to_string()),
            video_bitrate: Some("24M".to_string()),
            audio_codec: Some("aac".to_string()),
            audio_bitrate: Some("192k".to_string()),
            max_fps: Some(60),
            audio_enabled: !matches!(options.audio.mode, AudioMode::Off),
            audio_device: None,
        },
        RecordingEncodingPreset::SmallFile => ResolvedRecordingOptions {
            container_extension: DEFAULT_RECORDING_EXTENSION.to_string(),
            encode_resolution: scale_resolution(options.size, source_width, source_height)
                .map(|(width, height)| format!("{width}x{height}")),
            video_codec: Some("h264".to_string()),
            video_bitrate: Some("6M".to_string()),
            audio_codec: Some("aac".to_string()),
            audio_bitrate: Some("128k".to_string()),
            max_fps: Some(30),
            audio_enabled: !matches!(options.audio.mode, AudioMode::Off),
            audio_device: None,
        },
    };

    match normalized_audio_mode(options.audio.mode) {
        AudioMode::Off | AudioMode::Desktop => {}
        AudioMode::Microphone => {
            resolved.audio_device = options.audio.microphone_device.clone();
            if resolved.audio_device.is_none() {
                return Err(RecordError::UnsupportedAudioMode(
                    "microphone mode requires a microphone device".to_string(),
                ));
            }
        }
        AudioMode::Both => unreachable!("normalized audio mode should remove both"),
    }

    if let Some(advanced) = options.advanced.as_ref() {
        if let Some(container) = advanced.container.as_ref() {
            resolved.container_extension = normalize_extension(container);
        }
        if let Some(codec) = advanced.video_codec.as_ref() {
            resolved.video_codec = Some(codec.clone());
        }
        if let Some(bitrate) = advanced.video_bitrate.as_ref() {
            resolved.video_bitrate = Some(bitrate.clone());
        }
        if let Some(codec) = advanced.audio_codec.as_ref() {
            resolved.audio_codec = Some(codec.clone());
        }
        if let Some(bitrate) = advanced.audio_bitrate.as_ref() {
            resolved.audio_bitrate = Some(bitrate.clone());
        }
        if let Some(fps) = advanced.fps {
            resolved.max_fps = Some(fps.max(1));
        }
    }

    Ok(resolved)
}

fn normalize_extension(value: &str) -> String {
    let trimmed = value.trim().trim_start_matches('.');
    if trimmed.is_empty() {
        DEFAULT_RECORDING_EXTENSION.to_string()
    } else {
        trimmed.to_string()
    }
}

fn normalized_audio_mode(mode: AudioMode) -> AudioMode {
    match mode {
        AudioMode::Both => AudioMode::Desktop,
        other => other,
    }
}

fn scale_resolution(
    size: RecordingSize,
    source_width: u32,
    source_height: u32,
) -> Option<(u32, u32)> {
    match size {
        RecordingSize::Native => None,
        RecordingSize::Half => Some((
            even_dimension((source_width / 2).max(2)),
            even_dimension((source_height / 2).max(2)),
        )),
        RecordingSize::Fit1080p => fit_box(source_width, source_height, 1920, 1080),
        RecordingSize::Fit720p => fit_box(source_width, source_height, 1280, 720),
    }
}

fn command_available(command: &str) -> bool {
    let path = std::env::var_os("PATH");
    let Some(path) = path else {
        return false;
    };
    std::env::split_paths(&path).any(|segment| segment.join(command).is_file())
}

fn parse_pactl_source(line: &str) -> Option<AudioSource> {
    let mut columns = line.split_whitespace();
    let _index = columns.next()?;
    let name = columns.next()?.trim().to_string();
    if name.is_empty() {
        return None;
    }
    Some(AudioSource { name })
}

fn fit_box(
    source_width: u32,
    source_height: u32,
    max_width: u32,
    max_height: u32,
) -> Option<(u32, u32)> {
    if source_width == 0 || source_height == 0 {
        return None;
    }
    let width_ratio = max_width as f64 / source_width as f64;
    let height_ratio = max_height as f64 / source_height as f64;
    let ratio = width_ratio.min(height_ratio).min(1.0);
    let width = even_dimension(((source_width as f64) * ratio).round() as u32);
    let height = even_dimension(((source_height as f64) * ratio).round() as u32);
    Some((width.max(2), height.max(2)))
}

fn even_dimension(value: u32) -> u32 {
    if value <= 2 {
        return 2;
    }
    if value.is_multiple_of(2) {
        value
    } else {
        value - 1
    }
}

fn parse_geometry(geometry: &str) -> Result<RecordGeometry, RecordError> {
    let (position, size) = geometry
        .split_once(' ')
        .ok_or_else(|| RecordError::InvalidGeometry(geometry.to_string()))?;
    let (x, y) = position
        .split_once(',')
        .ok_or_else(|| RecordError::InvalidGeometry(geometry.to_string()))?;
    let (width, height) = size
        .split_once('x')
        .ok_or_else(|| RecordError::InvalidGeometry(geometry.to_string()))?;
    Ok(RecordGeometry {
        x: x.trim()
            .parse()
            .map_err(|_| RecordError::InvalidGeometry(geometry.to_string()))?,
        y: y.trim()
            .parse()
            .map_err(|_| RecordError::InvalidGeometry(geometry.to_string()))?,
        width: width
            .trim()
            .parse()
            .map_err(|_| RecordError::InvalidGeometry(geometry.to_string()))?,
        height: height
            .trim()
            .parse()
            .map_err(|_| RecordError::InvalidGeometry(geometry.to_string()))?,
    })
}

fn now_millis() -> Result<u64, RecordError> {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|err| RecordError::Metadata(err.to_string()))?;
    Ok(now.as_millis() as u64)
}

fn extract_thumbnail(video_path: &Path, thumbnail_path: &Path) -> Result<(), RecordError> {
    let status = Command::new("ffmpeg")
        .args([
            "-y",
            "-i",
            video_path.to_string_lossy().as_ref(),
            "-vframes",
            "1",
            "-vf",
            &format!("scale={RECORDING_THUMBNAIL_WIDTH}:-1"),
            thumbnail_path.to_string_lossy().as_ref(),
        ])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map_err(|source| RecordError::SpawnFailed {
            command: "ffmpeg".to_string(),
            source,
        })?;
    if status.success() {
        Ok(())
    } else {
        Err(RecordError::CommandFailed {
            command: "ffmpeg".to_string(),
            message: format!("exit status {:?}", status.code()),
        })
    }
}

fn write_recording_placeholder_thumbnail(thumbnail_path: &Path) -> Result<(), RecordError> {
    let thumbnail = RgbaImage::from_pixel(
        RECORDING_THUMBNAIL_WIDTH,
        RECORDING_THUMBNAIL_HEIGHT,
        Rgba([24, 24, 24, 255]),
    );
    thumbnail
        .save(thumbnail_path)
        .map_err(|err| RecordError::Metadata(err.to_string()))
}

#[derive(Debug, Clone, Copy)]
struct VideoMetadata {
    width: u32,
    height: u32,
    duration_ms: u64,
    file_size_bytes: u64,
}

fn probe_video_metadata(path: &Path) -> Result<VideoMetadata, RecordError> {
    let output = Command::new("ffprobe")
        .args([
            "-v",
            "error",
            "-select_streams",
            "v:0",
            "-show_entries",
            "stream=width,height",
            "-show_entries",
            "format=duration,size",
            "-of",
            "json",
            path.to_string_lossy().as_ref(),
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()
        .map_err(|source| RecordError::SpawnFailed {
            command: "ffprobe".to_string(),
            source,
        })?;
    if !output.status.success() {
        return Err(RecordError::CommandFailed {
            command: "ffprobe".to_string(),
            message: format!("exit status {:?}", output.status.code()),
        });
    }
    let parsed: serde_json::Value = serde_json::from_slice(&output.stdout)
        .map_err(|err| RecordError::Metadata(err.to_string()))?;
    let stream = parsed
        .get("streams")
        .and_then(serde_json::Value::as_array)
        .and_then(|items| items.first())
        .ok_or_else(|| RecordError::Metadata("missing video stream".to_string()))?;
    let format = parsed
        .get("format")
        .ok_or_else(|| RecordError::Metadata("missing format".to_string()))?;
    let width = stream
        .get("width")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(0) as u32;
    let height = stream
        .get("height")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(0) as u32;
    let duration_secs = format
        .get("duration")
        .and_then(serde_json::Value::as_str)
        .and_then(|value| value.parse::<f64>().ok())
        .unwrap_or(0.0);
    let file_size_bytes = format
        .get("size")
        .and_then(serde_json::Value::as_str)
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(0);
    Ok(VideoMetadata {
        width: width.max(1),
        height: height.max(1),
        duration_ms: (duration_secs * 1000.0).round() as u64,
        file_size_bytes,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fit_box_keeps_aspect_ratio_inside_target() {
        assert_eq!(fit_box(3840, 2160, 1920, 1080), Some((1920, 1080)));
        assert_eq!(fit_box(3000, 2000, 1280, 720), Some((1080, 720)));
    }

    #[test]
    fn scale_resolution_half_rounds_to_even_dimensions() {
        assert_eq!(
            scale_resolution(RecordingSize::Half, 2559, 1439),
            Some((1278, 718))
        );
    }

    #[test]
    fn parse_geometry_reads_slurp_format() {
        assert_eq!(
            parse_geometry("30,40 300x200").expect("geometry"),
            RecordGeometry {
                x: 30,
                y: 40,
                width: 300,
                height: 200,
            }
        );
    }

    #[test]
    fn resolve_recording_options_normalizes_audio_mix_to_desktop() {
        let resolved = resolve_recording_options(
            &RecordingOptions {
                audio: AudioConfig {
                    mode: AudioMode::Both,
                    microphone_device: Some("mic".into()),
                },
                ..RecordingOptions::default()
            },
            1920,
            1080,
        )
        .expect("both audio should normalize");
        assert!(resolved.audio_enabled);
        assert_eq!(resolved.audio_device, None);
    }

    #[test]
    fn recording_backend_requirement_message_mentions_backend() {
        assert!(recording_backend_requirement_message().contains(RECORDING_BACKEND_COMMAND));
    }

    #[test]
    fn write_recording_placeholder_thumbnail_creates_png() {
        let thumbnail_path = std::env::temp_dir().join(format!(
            "jjaeng-recording-thumb-{}.png",
            now_millis().expect("timestamp")
        ));
        write_recording_placeholder_thumbnail(&thumbnail_path).expect("placeholder thumbnail");
        let image = image::open(&thumbnail_path).expect("thumbnail image");
        assert_eq!(image.width(), RECORDING_THUMBNAIL_WIDTH);
        assert_eq!(image.height(), RECORDING_THUMBNAIL_HEIGHT);
        let _ = std::fs::remove_file(thumbnail_path);
    }
}
