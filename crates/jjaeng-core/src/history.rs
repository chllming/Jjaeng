use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use image::imageops::FilterType;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::capture::CaptureArtifact;
use crate::identity::APP_SLUG;
use crate::recording::RecordArtifact;

const HISTORY_MANIFEST_FILE: &str = "history.json";
const HISTORY_IMAGE_DIR: &str = "history/images";
const HISTORY_VIDEO_DIR: &str = "history/videos";
const HISTORY_THUMBNAIL_DIR: &str = "thumbnails";
const DEFAULT_HISTORY_LIMIT: usize = 48;
const THUMBNAIL_WIDTH: u32 = 320;
const THUMBNAIL_HEIGHT: u32 = 200;

#[derive(Debug, Error)]
pub enum HistoryError {
    #[error("missing HOME environment variable")]
    MissingHomeDirectory,
    #[error("history entry id is empty")]
    MissingEntryId,
    #[error("history entry not found for {0}")]
    EntryNotFound(String),
    #[error("history entry {0} is not a screenshot")]
    NotScreenshot(String),
    #[error("io error: {0}")]
    Io(#[from] io::Error),
    #[error("manifest parse error: {0}")]
    Manifest(#[from] serde_json::Error),
    #[error("image error: {0}")]
    Image(#[from] image::ImageError),
}

pub type HistoryResult<T> = std::result::Result<T, HistoryError>;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum HistoryEntryKind {
    #[default]
    Screenshot,
    Recording,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HistoryEntry {
    #[serde(default)]
    pub kind: HistoryEntryKind,
    #[serde(alias = "capture_id", alias = "recording_id")]
    pub entry_id: String,
    #[serde(alias = "image_path")]
    pub media_path: PathBuf,
    pub thumbnail_path: PathBuf,
    pub width: u32,
    pub height: u32,
    pub created_at: u64,
    #[serde(default)]
    pub saved_path: Option<PathBuf>,
    #[serde(default)]
    pub duration_ms: Option<u64>,
    #[serde(default)]
    pub file_size_bytes: Option<u64>,
}

impl HistoryEntry {
    pub fn display_thumbnail_path(&self) -> &Path {
        if self.thumbnail_path.exists() {
            &self.thumbnail_path
        } else {
            &self.media_path
        }
    }

    pub fn is_screenshot(&self) -> bool {
        matches!(self.kind, HistoryEntryKind::Screenshot)
    }

    pub fn is_recording(&self) -> bool {
        matches!(self.kind, HistoryEntryKind::Recording)
    }

    pub fn media_extension(&self) -> Option<&str> {
        self.media_path.extension().and_then(|value| value.to_str())
    }

    pub fn to_capture_artifact(&self) -> Option<CaptureArtifact> {
        if !self.is_screenshot() {
            return None;
        }
        Some(CaptureArtifact {
            capture_id: self.entry_id.clone(),
            temp_path: self.media_path.clone(),
            width: self.width,
            height: self.height,
            screen_x: 0,
            screen_y: 0,
            screen_width: self.width,
            screen_height: self.height,
            created_at: self.created_at,
        })
    }
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct HistoryManifest {
    #[serde(default)]
    entries: Vec<HistoryEntry>,
}

#[derive(Debug, Clone)]
pub struct HistoryService {
    manifest_path: PathBuf,
    image_dir: PathBuf,
    video_dir: PathBuf,
    thumbnail_dir: PathBuf,
    limit: usize,
}

impl HistoryService {
    pub fn with_default_paths() -> HistoryResult<Self> {
        let home = std::env::var_os("HOME")
            .map(PathBuf::from)
            .ok_or(HistoryError::MissingHomeDirectory)?;
        let state_root = std::env::var_os("XDG_STATE_HOME")
            .map(PathBuf::from)
            .filter(|path| !path.as_os_str().is_empty())
            .unwrap_or_else(|| home.join(".local/state"));
        let cache_root = std::env::var_os("XDG_CACHE_HOME")
            .map(PathBuf::from)
            .filter(|path| !path.as_os_str().is_empty())
            .unwrap_or_else(|| home.join(".cache"));

        Self::with_paths(
            state_root.join(APP_SLUG),
            cache_root.join(APP_SLUG),
            DEFAULT_HISTORY_LIMIT,
        )
    }

    pub fn with_paths(
        state_root: PathBuf,
        cache_root: PathBuf,
        limit: usize,
    ) -> HistoryResult<Self> {
        let manifest_path = state_root.join(HISTORY_MANIFEST_FILE);
        let image_dir = state_root.join(HISTORY_IMAGE_DIR);
        let video_dir = state_root.join(HISTORY_VIDEO_DIR);
        let thumbnail_dir = cache_root.join(HISTORY_THUMBNAIL_DIR);

        fs::create_dir_all(&image_dir)?;
        fs::create_dir_all(&video_dir)?;
        fs::create_dir_all(&thumbnail_dir)?;

        Ok(Self {
            manifest_path,
            image_dir,
            video_dir,
            thumbnail_dir,
            limit: limit.max(1),
        })
    }

    pub fn list_entries(&self) -> HistoryResult<Vec<HistoryEntry>> {
        let mut manifest = self.load_manifest()?;
        let original_len = manifest.entries.len();
        manifest.entries.retain(|entry| entry.media_path.exists());
        if manifest.entries.len() != original_len {
            self.save_manifest(&manifest)?;
        }
        Ok(manifest.entries)
    }

    pub fn record_capture(&self, artifact: &CaptureArtifact) -> HistoryResult<HistoryEntry> {
        validate_entry_id(&artifact.capture_id)?;

        let media_path = self.image_path_for_entry(&artifact.capture_id);
        let thumbnail_path = self.thumbnail_path_for_entry(&artifact.capture_id);
        let mut manifest = self.load_manifest()?;
        let saved_path = manifest
            .entries
            .iter()
            .find(|entry| entry.entry_id == artifact.capture_id)
            .and_then(|entry| entry.saved_path.clone());

        copy_if_different(&artifact.temp_path, &media_path)?;
        write_thumbnail(&media_path, &thumbnail_path)?;

        let entry = HistoryEntry {
            kind: HistoryEntryKind::Screenshot,
            entry_id: artifact.capture_id.clone(),
            media_path,
            thumbnail_path,
            width: artifact.width,
            height: artifact.height,
            created_at: artifact.created_at,
            saved_path,
            duration_ms: None,
            file_size_bytes: None,
        };

        manifest
            .entries
            .retain(|item| item.entry_id != artifact.capture_id);
        manifest.entries.insert(0, entry.clone());
        self.trim_manifest(&mut manifest);
        self.save_manifest(&manifest)?;
        Ok(entry)
    }

    pub fn record_recording(&self, artifact: &RecordArtifact) -> HistoryResult<HistoryEntry> {
        validate_entry_id(&artifact.recording_id)?;

        let extension = artifact
            .output_path
            .extension()
            .and_then(|value| value.to_str())
            .unwrap_or("mp4");
        let media_path = self.video_path_for_entry(&artifact.recording_id, extension);
        let thumbnail_path = self.thumbnail_path_for_entry(&artifact.recording_id);
        let mut manifest = self.load_manifest()?;
        let saved_path = manifest
            .entries
            .iter()
            .find(|entry| entry.entry_id == artifact.recording_id)
            .and_then(|entry| entry.saved_path.clone());

        copy_if_different(&artifact.output_path, &media_path)?;
        copy_if_different(&artifact.thumbnail_path, &thumbnail_path)?;

        let entry = HistoryEntry {
            kind: HistoryEntryKind::Recording,
            entry_id: artifact.recording_id.clone(),
            media_path,
            thumbnail_path,
            width: artifact.width,
            height: artifact.height,
            created_at: artifact.created_at,
            saved_path,
            duration_ms: Some(artifact.duration_ms),
            file_size_bytes: Some(artifact.file_size_bytes),
        };

        manifest
            .entries
            .retain(|item| item.entry_id != artifact.recording_id);
        manifest.entries.insert(0, entry.clone());
        self.trim_manifest(&mut manifest);
        self.save_manifest(&manifest)?;
        Ok(entry)
    }

    pub fn mark_saved(&self, entry_id: &str, saved_path: impl AsRef<Path>) -> HistoryResult<()> {
        validate_entry_id(entry_id)?;
        let mut manifest = self.load_manifest()?;
        let Some(entry) = manifest
            .entries
            .iter_mut()
            .find(|entry| entry.entry_id == entry_id)
        else {
            return Err(HistoryError::EntryNotFound(entry_id.to_string()));
        };

        entry.saved_path = Some(saved_path.as_ref().to_path_buf());
        self.save_manifest(&manifest)
    }

    pub fn remove_entry(&self, entry_id: &str) -> HistoryResult<()> {
        validate_entry_id(entry_id)?;
        let mut manifest = self.load_manifest()?;
        let removed = manifest
            .entries
            .iter()
            .find(|entry| entry.entry_id == entry_id)
            .cloned()
            .ok_or_else(|| HistoryError::EntryNotFound(entry_id.to_string()))?;
        manifest.entries.retain(|entry| entry.entry_id != entry_id);
        self.save_manifest(&manifest)?;
        remove_if_exists(&removed.media_path)?;
        remove_if_exists(&removed.thumbnail_path)?;
        Ok(())
    }

    pub fn entry_artifact(&self, entry_id: &str) -> HistoryResult<CaptureArtifact> {
        validate_entry_id(entry_id)?;
        let manifest = self.load_manifest()?;
        let entry = manifest
            .entries
            .into_iter()
            .find(|entry| entry.entry_id == entry_id)
            .ok_or_else(|| HistoryError::EntryNotFound(entry_id.to_string()))?;
        entry
            .to_capture_artifact()
            .ok_or_else(|| HistoryError::NotScreenshot(entry_id.to_string()))
    }

    fn load_manifest(&self) -> HistoryResult<HistoryManifest> {
        if !self.manifest_path.exists() {
            return Ok(HistoryManifest::default());
        }

        let contents = fs::read_to_string(&self.manifest_path)?;
        Ok(serde_json::from_str(&contents)?)
    }

    fn save_manifest(&self, manifest: &HistoryManifest) -> HistoryResult<()> {
        if let Some(parent) = self.manifest_path.parent() {
            fs::create_dir_all(parent)?;
        }

        let tmp_path = self.manifest_path.with_extension("json.tmp");
        let encoded = serde_json::to_vec_pretty(manifest)?;
        fs::write(&tmp_path, encoded)?;
        fs::rename(tmp_path, &self.manifest_path)?;
        Ok(())
    }

    fn image_path_for_entry(&self, entry_id: &str) -> PathBuf {
        self.image_dir.join(format!("{entry_id}.png"))
    }

    fn video_path_for_entry(&self, entry_id: &str, extension: &str) -> PathBuf {
        self.video_dir.join(format!("{entry_id}.{extension}"))
    }

    fn thumbnail_path_for_entry(&self, entry_id: &str) -> PathBuf {
        self.thumbnail_dir.join(format!("{entry_id}.png"))
    }

    fn trim_manifest(&self, manifest: &mut HistoryManifest) {
        if manifest.entries.len() <= self.limit {
            return;
        }

        for entry in manifest.entries.drain(self.limit..) {
            let _ = remove_if_exists(&entry.media_path);
            let _ = remove_if_exists(&entry.thumbnail_path);
        }
    }
}

fn validate_entry_id(entry_id: &str) -> HistoryResult<()> {
    if entry_id.is_empty() {
        return Err(HistoryError::MissingEntryId);
    }
    Ok(())
}

fn copy_if_different(source: &Path, destination: &Path) -> HistoryResult<()> {
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent)?;
    }
    if source != destination {
        let _ = fs::remove_file(destination);
        fs::copy(source, destination)?;
    }
    Ok(())
}

fn remove_if_exists(path: &Path) -> HistoryResult<()> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(err) => Err(HistoryError::Io(err)),
    }
}

fn write_thumbnail(image_path: &Path, thumbnail_path: &Path) -> HistoryResult<()> {
    if let Some(parent) = thumbnail_path.parent() {
        fs::create_dir_all(parent)?;
    }
    let image = image::open(image_path)?;
    let thumbnail = image.resize(THUMBNAIL_WIDTH, THUMBNAIL_HEIGHT, FilterType::Lanczos3);
    thumbnail.save(thumbnail_path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn unique_test_dir(label: &str) -> PathBuf {
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("jjaeng-history-{label}-{suffix}"))
    }

    fn write_fixture_png(path: &Path) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        let image = image::DynamicImage::new_rgba8(16, 10);
        image.save(path).unwrap();
    }

    fn write_fixture_video(path: &Path) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(path, b"video").unwrap();
    }

    fn artifact(id: &str, temp_path: PathBuf) -> CaptureArtifact {
        CaptureArtifact {
            capture_id: id.to_string(),
            temp_path,
            width: 16,
            height: 10,
            screen_x: 0,
            screen_y: 0,
            screen_width: 16,
            screen_height: 10,
            created_at: 123,
        }
    }

    fn recording(id: &str, output_path: PathBuf, thumbnail_path: PathBuf) -> RecordArtifact {
        RecordArtifact {
            recording_id: id.to_string(),
            output_path,
            thumbnail_path,
            width: 1920,
            height: 1080,
            duration_ms: 3456,
            file_size_bytes: 1024,
            created_at: 456,
            audio_config: Default::default(),
        }
    }

    #[test]
    fn record_capture_persists_entry_and_thumbnail() {
        let state_root = unique_test_dir("record-state");
        let cache_root = unique_test_dir("record-cache");
        let temp_path = state_root.join("source.png");
        fs::create_dir_all(&state_root).unwrap();
        write_fixture_png(&temp_path);
        let service = HistoryService::with_paths(state_root.clone(), cache_root, 8).unwrap();

        let entry = service
            .record_capture(&artifact("capture-1", temp_path))
            .unwrap();

        assert!(entry.media_path.exists());
        assert!(entry.thumbnail_path.exists());
        assert!(entry.is_screenshot());
        assert_eq!(service.list_entries().unwrap(), vec![entry]);
    }

    #[test]
    fn record_recording_persists_video_and_thumbnail() {
        let state_root = unique_test_dir("recording-state");
        let cache_root = unique_test_dir("recording-cache");
        let video_path = state_root.join("source.mp4");
        let thumbnail_path = state_root.join("source-thumb.png");
        write_fixture_video(&video_path);
        write_fixture_png(&thumbnail_path);
        let service = HistoryService::with_paths(state_root.clone(), cache_root, 8).unwrap();

        let entry = service
            .record_recording(&recording("recording-1", video_path, thumbnail_path))
            .unwrap();

        assert!(entry.media_path.exists());
        assert!(entry.thumbnail_path.exists());
        assert!(entry.is_recording());
        assert_eq!(entry.duration_ms, Some(3456));
    }

    #[test]
    fn mark_saved_updates_manifest() {
        let state_root = unique_test_dir("mark-saved-state");
        let cache_root = unique_test_dir("mark-saved-cache");
        let temp_path = state_root.join("source.png");
        fs::create_dir_all(&state_root).unwrap();
        write_fixture_png(&temp_path);
        let service = HistoryService::with_paths(state_root.clone(), cache_root, 8).unwrap();
        service
            .record_capture(&artifact("capture-2", temp_path))
            .unwrap();

        service
            .mark_saved("capture-2", PathBuf::from("/tmp/capture-2.png"))
            .unwrap();

        let entries = service.list_entries().unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(
            entries[0].saved_path.as_deref(),
            Some(Path::new("/tmp/capture-2.png"))
        );
    }

    #[test]
    fn record_capture_trims_entries_beyond_limit() {
        let state_root = unique_test_dir("trim-state");
        let cache_root = unique_test_dir("trim-cache");
        let service = HistoryService::with_paths(state_root.clone(), cache_root, 1).unwrap();

        for capture_id in ["capture-3", "capture-4"] {
            let temp_path = state_root.join(format!("{capture_id}-source.png"));
            write_fixture_png(&temp_path);
            service
                .record_capture(&artifact(capture_id, temp_path))
                .unwrap();
        }

        let entries = service.list_entries().unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].entry_id, "capture-4");
    }

    #[test]
    fn remove_entry_purges_files_and_manifest() {
        let state_root = unique_test_dir("remove-state");
        let cache_root = unique_test_dir("remove-cache");
        let temp_path = state_root.join("source.png");
        fs::create_dir_all(&state_root).unwrap();
        write_fixture_png(&temp_path);
        let service = HistoryService::with_paths(state_root, cache_root, 8).unwrap();
        let entry = service
            .record_capture(&artifact("capture-5", temp_path))
            .unwrap();

        service.remove_entry("capture-5").unwrap();

        assert!(service.list_entries().unwrap().is_empty());
        assert!(!entry.media_path.exists());
        assert!(!entry.thumbnail_path.exists());
    }
}
