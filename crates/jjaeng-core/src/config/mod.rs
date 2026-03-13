use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::identity::{config_dir_candidates, APP_SLUG};
use crate::recording::{AudioMode, RecordingEncodingPreset, RecordingSize, RecordingTarget};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigPathError {
    MissingHomeDirectory,
}

const APP_CONFIG_FILE: &str = "config.json";

/// Application-level settings from `config.json`.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct AppConfig {
    #[serde(default)]
    pub ocr_language: Option<String>,
    #[serde(default)]
    pub screenshot_dir: Option<PathBuf>,
    #[serde(default)]
    pub recording_dir: Option<PathBuf>,
    #[serde(default)]
    pub recording_size: Option<RecordingSize>,
    #[serde(default)]
    pub recording_encoding_preset: Option<RecordingEncodingPreset>,
    #[serde(default)]
    pub recording_audio_mode: Option<AudioMode>,
    #[serde(default)]
    pub recording_mic_device: Option<String>,
    #[serde(default)]
    pub recording_target: Option<RecordingTarget>,
}

pub fn load_app_config() -> AppConfig {
    let (xdg_config_home, home) = config_env_dirs();
    load_app_config_with(xdg_config_home.as_deref(), home.as_deref())
}

fn load_app_config_with(xdg_config_home: Option<&Path>, home: Option<&Path>) -> AppConfig {
    let path = match existing_app_config_path(APP_CONFIG_FILE, xdg_config_home, home) {
        Ok(p) => p,
        Err(_) => return AppConfig::default(),
    };
    if !path.exists() {
        return AppConfig::default();
    }
    match std::fs::read_to_string(&path) {
        Ok(contents) => serde_json::from_str(&contents).unwrap_or_else(|err| {
            tracing::warn!(?err, ?path, "failed to parse config.json; using defaults");
            AppConfig::default()
        }),
        Err(err) => {
            tracing::warn!(?err, ?path, "failed to read config.json; using defaults");
            AppConfig::default()
        }
    }
}

pub fn existing_app_config_path(
    file_name: &str,
    xdg_config_home: Option<&Path>,
    home: Option<&Path>,
) -> Result<PathBuf, ConfigPathError> {
    let primary = app_config_path(APP_SLUG, file_name, xdg_config_home, home)?;
    if primary.exists() {
        return Ok(primary);
    }

    for candidate in config_dir_candidates() {
        if *candidate == APP_SLUG {
            continue;
        }
        let path = app_config_path(candidate, file_name, xdg_config_home, home)?;
        if path.exists() {
            return Ok(path);
        }
    }

    Ok(primary)
}

pub fn config_env_dirs() -> (Option<PathBuf>, Option<PathBuf>) {
    (
        std::env::var_os("XDG_CONFIG_HOME").map(PathBuf::from),
        std::env::var_os("HOME").map(PathBuf::from),
    )
}

pub fn app_config_path(
    app_dir: &str,
    file_name: &str,
    xdg_config_home: Option<&Path>,
    home: Option<&Path>,
) -> Result<PathBuf, ConfigPathError> {
    let mut path = config_root(xdg_config_home, home)?;
    path.push(app_dir);
    path.push(file_name);
    Ok(path)
}

fn config_root(
    xdg_config_home: Option<&Path>,
    home: Option<&Path>,
) -> Result<PathBuf, ConfigPathError> {
    if let Some(xdg) = xdg_config_home.filter(|path| !path.as_os_str().is_empty()) {
        return Ok(xdg.to_path_buf());
    }

    let home = home.ok_or(ConfigPathError::MissingHomeDirectory)?;
    Ok(home.join(".config"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_config_path_prefers_xdg_config_home() {
        let path = app_config_path(
            APP_SLUG,
            "theme.json",
            Some(Path::new("/tmp/config-root")),
            Some(Path::new("/tmp/home")),
        )
        .expect("path should resolve");

        assert_eq!(path, PathBuf::from("/tmp/config-root/jjaeng/theme.json"));
    }

    #[test]
    fn app_config_path_falls_back_to_home_dot_config() {
        let path = app_config_path(APP_SLUG, "theme.json", None, Some(Path::new("/tmp/home")))
            .expect("path should resolve");

        assert_eq!(path, PathBuf::from("/tmp/home/.config/jjaeng/theme.json"));
    }

    #[test]
    fn app_config_path_errors_when_home_missing_and_xdg_unset() {
        let error = app_config_path(APP_SLUG, "theme.json", None, None).unwrap_err();
        assert_eq!(error, ConfigPathError::MissingHomeDirectory);
    }
}
