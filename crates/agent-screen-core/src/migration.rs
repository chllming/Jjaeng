//! Safe, idempotent migration from the historical Jjaeng/ChalKak namespaces.
//!
//! Migration is deliberately additive: legacy files are copied into the canonical
//! tree only when the canonical counterpart is absent, and nothing is removed.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::identity::{APP_SLUG, LEGACY_APP_SLUG, UPSTREAM_SLUG};

const MIGRATION_MARKER: &str = ".namespace-migrated-v1";

pub fn migrate_legacy_state() -> io::Result<bool> {
    let Some(home) = std::env::var_os("HOME").map(PathBuf::from) else {
        return Ok(false);
    };
    let config_root = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| home.join(".config"));
    let state_root = std::env::var_os("XDG_STATE_HOME")
        .map(PathBuf::from)
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| home.join(".local/state"));
    let cache_root = std::env::var_os("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| home.join(".cache"));
    migrate_legacy_state_with_roots(&config_root, &state_root, &cache_root)
}

pub fn migrate_legacy_state_at(home: &Path) -> io::Result<bool> {
    let config_root = home.join(".config");
    let state_root = home.join(".local/state");
    let cache_root = home.join(".cache");
    migrate_legacy_state_with_roots(&config_root, &state_root, &cache_root)
}

fn migrate_legacy_state_with_roots(
    config_root: &Path,
    state_root: &Path,
    cache_root: &Path,
) -> io::Result<bool> {
    let marker = config_root.join(APP_SLUG).join(MIGRATION_MARKER);
    if marker.exists() {
        return Ok(false);
    }

    let mut changed = false;
    for root in [&config_root, &state_root, &cache_root] {
        let destination = root.join(APP_SLUG);
        for legacy in [LEGACY_APP_SLUG, UPSTREAM_SLUG] {
            let source = root.join(legacy);
            if source.exists() {
                changed |= copy_tree_if_missing(&source, &destination)?;
            }
        }
    }

    rewrite_manifest_paths(&state_root.join(APP_SLUG), state_root, cache_root)?;
    fs::create_dir_all(marker.parent().expect("marker has parent"))?;
    fs::write(marker, b"agent-screen namespace migration complete\n")?;
    Ok(changed)
}

fn copy_tree_if_missing(source: &Path, destination: &Path) -> io::Result<bool> {
    let mut changed = false;
    if source.is_dir() {
        fs::create_dir_all(destination)?;
        for entry in fs::read_dir(source)? {
            let entry = entry?;
            let target = destination.join(entry.file_name());
            changed |= copy_tree_if_missing(&entry.path(), &target)?;
        }
    } else if !destination.exists() {
        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::copy(source, destination)?;
        changed = true;
    }
    Ok(changed)
}

fn rewrite_manifest_paths(
    canonical_state: &Path,
    state_root: &Path,
    cache_root: &Path,
) -> io::Result<()> {
    let manifest = canonical_state.join("history.json");
    if !manifest.exists() {
        return Ok(());
    }
    let mut contents = fs::read_to_string(&manifest)?;
    for legacy in [LEGACY_APP_SLUG, UPSTREAM_SLUG] {
        let old_state = state_root.join(legacy).to_string_lossy().into_owned();
        let old_cache = cache_root.join(legacy).to_string_lossy().into_owned();
        let new_state = canonical_state.to_string_lossy();
        let new_cache = cache_root.join(APP_SLUG).to_string_lossy().into_owned();
        contents = contents.replace(&old_state, &new_state);
        contents = contents.replace(&old_cache, &new_cache);
    }
    fs::write(manifest, contents)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migration_copies_legacy_config_without_deleting_it() {
        let root =
            std::env::temp_dir().join(format!("agent-screen-migration-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join(".config/jjaeng")).unwrap();
        fs::write(
            root.join(".config/jjaeng/theme.json"),
            b"{\"mode\":\"dark\"}",
        )
        .unwrap();
        migrate_legacy_state_at(&root).unwrap();
        assert!(root.join(".config/jjaeng/theme.json").exists());
        assert!(root.join(".config/agent-screen/theme.json").exists());
        assert!(root
            .join(".config/agent-screen/.namespace-migrated-v1")
            .exists());
        let _ = fs::remove_dir_all(root);
    }
}
