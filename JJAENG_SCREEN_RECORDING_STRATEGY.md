# Jjaeng screen recording — implementation status

This document now reflects the code that is actually in the repository as of 2026-03-13. The original strategy was broader than the first delivered implementation, so this file is split into:

- what is implemented now
- what was intentionally deferred
- the next steps that still make sense

## Current status

Jjaeng now has a working screen recording flow built into the app:

- Two recording backends are implemented: `gpu-screen-recorder` (preferred) and `wl-screenrec` (fallback).
- Recordings can be started for fullscreen, region, and window targets.
- Recordings can be stopped from the daemon-managed app state.
- A compact recording bar (prompt mode) allows picking audio source, scale, and quality before recording starts.
- Completed recordings generate thumbnails and metadata.
- Recordings appear in history together with screenshots.
- History supports `All`, `Images`, and `Videos` filtering.
- Launchpad and CLI can start and stop recordings.
- A recording result window shows on stop with Save, Copy Path, and Open actions.

The implementation is deliberately MVP-shaped. It prioritizes a stable mixed-media workflow over fully exposing every idea from the original strategy.

---

## Implemented architecture

### Recording module

The recording implementation currently lives in a single module:

```text
crates/jjaeng-core/src/recording/mod.rs
```

It was not split into `session.rs` and `audio.rs` yet.

The module currently owns:

- `RecordBackend`
- `SystemRecordBackend` (wl-screenrec)
- `GpuScreenRecorderBackend` (gpu-screen-recorder)
- `RecordingTarget`
- `RecordingSize`
- `RecordingEncodingPreset`
- `AudioMode`
- `AudioConfig`
- `RecordingAdvancedOverrides`
- `RecordingOptions`
- `RecordingRequest`
- `ResolvedRecordingOptions`
- `RecordGeometry`
- `RecordingHandle`
- `RecordArtifact`
- `RecordError`

### Actual public recording types

```rust
pub enum RecordingTarget {
    Fullscreen,
    Region,
    Window,
}

pub enum RecordingSize {
    Native,
    Half,
    Fit1080p,
    Fit720p,
}

pub enum RecordingEncodingPreset {
    Standard,
    HighQuality,
    SmallFile,
}

pub enum AudioMode {
    Off,
    Desktop,
    Microphone,
    Both,
}

pub struct AudioConfig {
    pub mode: AudioMode,
    pub microphone_device: Option<String>,
}

pub struct RecordingOptions {
    pub size: RecordingSize,
    pub encoding: RecordingEncodingPreset,
    pub audio: AudioConfig,
    pub advanced: Option<RecordingAdvancedOverrides>,
}

pub struct RecordingRequest {
    pub target: RecordingTarget,
    pub options: RecordingOptions,
}
```

Important reality:

- `AudioMode::Both` exists in the type model
- the backend still rejects it as unsupported
- the launchpad and CLI help therefore only expose `off`, `desktop`, and `mic`

This keeps the public UI on working options while leaving room for the combined-source path later.

### Backend behavior

Two backends are implemented with automatic fallback. The preferred backend is determined by `preferred_record_backend_kind()`, which checks availability in order: `gpu-screen-recorder` first, then `wl-screenrec`.

**`SystemRecordBackend`** shells out to `wl-screenrec`:

- fullscreen recording uses `-o <monitor_name>`
- region and window recording use `-g <geometry>`
- size presets resolve into `--encode-resolution`
- encoding presets resolve into codec/bitrate/fps settings
- audio uses `--audio` and optional `--audio-device`

**`GpuScreenRecorderBackend`** shells out to `gpu-screen-recorder`:

- fullscreen recording uses `-w <monitor_name>`
- region and window recording use `-w <geometry>`
- quality presets map to `-q` levels (medium/high/very_high)
- audio uses `-a <device>` with optional codec and bitrate

Stopping a recording is implemented by sending `SIGINT` through:

```text
kill -INT <pid>
```

The stop flow then:

1. waits for the recorder to exit (with a 10-second timeout; escalates to SIGKILL if needed)
2. validates the output file exists and is non-empty
3. extracts a thumbnail with `ffmpeg`
4. probes width, height, and duration with `ffprobe`
5. builds a `RecordArtifact`

### Selection reuse

Recording target selection reuses the existing screenshot selection plumbing in `crates/jjaeng-core/src/capture/mod.rs`.

The following helpers were added so recording can share the same behavior:

- `focused_monitor_target()`
- `select_region_geometry()`
- `select_window_geometry()`

This keeps `slurp` / Hyprland geometry resolution aligned between screenshots and recordings.

---

## State machine and IPC

### State machine

The app state machine now includes:

```rust
pub enum AppState {
    Idle,
    Preview,
    Editor,
    Recording,
}
```

And the new events:

```rust
pub enum AppEvent {
    StartRecording,
    StopRecording,
    // existing events omitted
}
```

Implemented transition rules:

- `Idle + StartRecording -> Recording`
- `Recording + StopRecording -> Idle`

Recording is intentionally mutually exclusive with Preview and Editor.

### Remote commands

`RemoteCommand` now includes:

```rust
StartRecording(RecordingRequest),
StopRecording,
```

This is what both CLI startup parsing and daemon fallback dispatch use.

### Status snapshot

`StatusSnapshot` now includes recording-specific fields:

```rust
pub struct StatusSnapshot {
    pub recording: bool,
    pub recording_duration_ms: Option<u64>,
    pub recording_id: Option<String>,
    // existing fields omitted
}
```

The originally proposed `recording_path` field is not implemented.

---

## App runtime integration

The GTK app now owns recording lifecycle state in `crates/jjaeng-ui/src/app/mod.rs`.

Actual runtime pieces:

- `RecordingRuntimeState`
- `ActiveRecording`
- start-recording closure
- stop-recording closure
- a 1-second `glib::timeout_add_local` timer for elapsed duration

### Start flow in the current app

1. Build a `RecordingRequest`
2. Resolve target geometry if needed
3. Spawn `recording::start_recording(...)` on a worker
4. Transition `Idle -> Recording`
5. Start the elapsed-time timer
6. Update status and send notification

### Stop flow in the current app

1. Take the active handle from runtime state
2. Call `recording::stop_recording(...)`
3. Persist the artifact to history if available
4. Fall back to storage-only save if history persistence fails
5. Transition `Recording -> Idle`
6. stop the timer
7. remove temp files after persistence
8. update status and send notification

This is daemon-managed. Recording state is not stored in `RuntimeSession`, which remains screenshot-oriented.

---

## Launchpad implementation

The launchpad now includes a dedicated recording section in:

```text
crates/jjaeng-ui/src/app/launchpad.rs
```

Implemented controls:

- `Record Full`
- `Record Region`
- `Record Window`
- `Stop Recording`
- `Size` combo: `Native`, `Half`, `1080p`, `720p`
- `Encoding` combo: `Standard`, `High Quality`, `Small File`
- `Audio` combo: `No Audio`, `Desktop`, `Mic`
- `Mic` text entry shown only for microphone mode

This is not the segmented-toggle control strip described in the original strategy. The current implementation uses compact GTK combos because they are simpler and stable in the existing launchpad surface.

The launchpad does not yet enumerate PipeWire/Pulse sources. Microphone input is currently a freeform source-name text field.

---

## CLI implementation

The CLI and startup parser currently support:

```text
jjaeng --record-full
jjaeng --record-region
jjaeng --record-window
jjaeng --stop-recording

jjaeng --record-size=native|half|1080p|720p
jjaeng --record-encoding=standard|quality|small
jjaeng --record-audio=off|desktop|mic
jjaeng --record-mic=<source_name>
```

Implemented parsing lives in:

```text
crates/jjaeng-ui/src/app/runtime_support/startup.rs
crates/jjaeng-cli/src/main.rs
```

Not implemented from the original strategy:

- `--record-format`
- `--record-codec`
- `--record-bitrate`
- `--record-audio-codec`
- `--record-audio-bitrate`
- `--record-fps`

Those remain future work.

---

## Storage implementation

Storage now supports recordings alongside screenshots.

### Actual storage changes

`crates/jjaeng-core/src/storage/mod.rs` now includes:

- `videos_dir`
- `allocate_recording_target_path_with_extension(...)`
- `save_recording(...)`
- `save_recording_path(...)`
- `create_temp_recording(...)`

Default recording save location:

```text
$HOME/Videos/jjaeng
```

The storage service now resolves both pictures and videos paths during app lifecycle bootstrap.

---

## History implementation

This is the largest behavioral change from the original app.

### Actual history model

History is no longer screenshot-only. `crates/jjaeng-core/src/history.rs` now stores a mixed manifest using:

```rust
pub enum HistoryEntryKind {
    Screenshot,
    Recording,
}

pub struct HistoryEntry {
    pub kind: HistoryEntryKind,
    pub entry_id: String,
    pub media_path: PathBuf,
    pub thumbnail_path: PathBuf,
    pub width: u32,
    pub height: u32,
    pub created_at: u64,
    pub saved_path: Option<PathBuf>,
    pub duration_ms: Option<u64>,
    pub file_size_bytes: Option<u64>,
}
```

This replaced the older screenshot-specific manifest model.

### Actual history storage layout

History now maintains separate on-disk buckets under the app history directory:

- `history/images`
- `history/videos`
- `history/thumbnails`

### Actual history APIs

Implemented service methods now include:

- `record_capture(...)`
- `record_recording(...)`
- `mark_saved(...)`
- `remove_entry(...)`
- `list_entries(...)`

### History UI behavior

The history window now renders screenshots and recordings together in one descending timeline.

Implemented filters:

- `All`
- `Images`
- `Videos`

Recording entries currently provide:

- `Save`
- `Copy Path`
- `Open`

Screenshot entries keep:

- `Save`
- `Copy`
- `Open`

Visual distinction is currently a media badge (`IMAGE` / `VIDEO`) rather than a play-icon overlay.

This is intentionally simpler than a custom mixed-media card system and fits the existing history grid.

---

## Thumbnail and metadata generation

When a recording stops:

- `ffmpeg` extracts a thumbnail PNG
- `ffprobe` reads video metadata

This is implemented now and covered by unit tests around recording artifact generation and history persistence.

The thumbnail is what the history grid renders for recording entries.

---

## Config implementation

`crates/jjaeng-core/src/config/mod.rs` currently supports these recording fields:

```json
{
  "recording_dir": "~/Videos/jjaeng",
  "recording_size": "native",
  "recording_encoding_preset": "standard",
  "recording_audio_mode": "off",
  "recording_mic_device": null
}
```

Also present in the config struct:

```json
{
  "recording_target": "fullscreen"
}
```

That field is currently carried in config but not used as a primary UX surface.

Not implemented from the original strategy:

- `recording_backend`
- advanced codec/container/bitrate/fps override fields
- `recording_allow_modern_codecs`

---

## What was deferred

These parts of the original strategy are still good ideas, but they are not implemented yet.

### Audio follow-up

Not implemented yet:

- `recording/audio.rs`
- PipeWire/Pulse source enumeration
- combined desktop + mic capture
- temporary mixed-source lifecycle management

The type model still has `AudioMode::Both`, but the backend rejects it and the public UI does not expose it.

### Backend follow-up

Implemented:

- `gpu-screen-recorder` as preferred backend with `wl-screenrec` fallback

Not implemented yet:

- `wf-recorder` fallback backend
- backend selection in config
- backend-specific capability detection

### UX follow-up

Implemented:

- compact recording bar with icon controls for target, audio, scale, quality, and record/pause/stop
- separate system-audio and microphone toggles with adjacent source dropdown chevrons
- live elapsed timer during active recording
- recording result window with Save, Copy Path, Open, and Close actions

Not implemented yet:

- richer video tile overlays

### Recovery and system integration

Not implemented yet:

- orphaned recording recovery after daemon crash
- Waybar status script updates for recording state
- packaging changes for `wl-screenrec` runtime dependency

---

## Recommended next steps

The current implementation is usable. The next work should improve the weakest real edges rather than reopening the whole design.

1. Split `crates/jjaeng-core/src/recording/mod.rs` into `mod.rs`, `session.rs`, and `audio.rs`.
2. Implement audio source enumeration and replace the mic text field with a real source picker.
3. Implement combined desktop + mic capture, then expose it in the launchpad and CLI help.
4. Add Waybar recording indicator support using the existing `StatusSnapshot.recording` and `recording_duration_ms` fields.
5. Add orphaned recording recovery in the daemon startup path.
6. Add backend abstraction depth only if a real `wf-recorder` fallback is still needed in practice.

---

## Summary

The repository now contains a real first-pass recording system, not just a plan:

- recording backend integration exists
- state machine and IPC support exist
- launchpad and CLI support exist
- recordings persist and show up in history
- mixed image/video history filtering exists

The main gaps are audio source handling, combined desktop+mic recording, fallback backends, and deeper system integration.
