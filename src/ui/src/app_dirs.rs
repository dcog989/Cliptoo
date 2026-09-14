use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

/// Resolved application directory paths.
///
///   config/state  (~/.config/Cliptoo/)      — settings, DB, logs
///   data          (~/.local/share/Cliptoo/) — full-resolution clipboard images
///   cache         (~/.cache/Cliptoo/)       — thumbnails, favicons (regenerable)
pub struct AppDirs {
    pub config_dir: PathBuf,
    pub data_dir: PathBuf,
    pub cache_dir: PathBuf,
    pub logs_dir: PathBuf,
    pub db_path: PathBuf,
    pub settings_path: PathBuf,
    pub images_dir: PathBuf,
    pub thumbnails_dir: PathBuf,
    pub favicons_dir: PathBuf,
}

impl AppDirs {
    /// Resolve XDG base directories via the `dirs` crate.
    pub fn resolve() -> Result<Self> {
        let config_home = dirs::config_dir().context("no config dir (unset HOME?)")?;
        let data_home = dirs::data_dir().context("no data dir (unset HOME?)")?;
        let cache_home = dirs::cache_dir().context("no cache dir (unset HOME?)")?;

        // Config/state — settings, DB, logs
        let config_dir = config_home.join("Cliptoo");
        let logs_dir = config_dir.join("Logs");

        // Data — full-resolution images are referenced by clip rows, so they
        // must survive a cache clear and live under `$XDG_DATA_HOME`.
        let data_dir = data_home.join("Cliptoo");
        let images_dir = data_dir.join("images");

        // Cache — thumbnails and favicons are derived and rebuilt on demand.
        let cache_dir = cache_home.join("Cliptoo");
        let thumbnails_dir = cache_dir.join("thumbnails");
        let favicons_dir = cache_dir.join("favicons");

        // Create all directories on first run
        for dir in &[
            &config_dir,
            &data_dir,
            &cache_dir,
            &logs_dir,
            &images_dir,
            &thumbnails_dir,
            &favicons_dir,
        ] {
            std::fs::create_dir_all(dir)?;
        }

        // One-time migration: thumbnails/favicons previously lived under the
        // data dir. Move existing files across so cached favicons (and their
        // failure counters) survive the split; they regenerate if the move
        // fails (e.g. data and cache on different filesystems).
        migrate_dir(&data_dir.join("thumbnails"), &thumbnails_dir);
        migrate_dir(&data_dir.join("favicons"), &favicons_dir);
        migrate_file(
            &data_dir.join(crate::favicon::FAVICON_FAILURES_FILE),
            &cache_dir.join(crate::favicon::FAVICON_FAILURES_FILE),
        );

        Ok(Self {
            db_path: config_dir.join("clips.db"),
            settings_path: config_dir.join("settings.json"),
            config_dir,
            data_dir,
            cache_dir,
            logs_dir,
            images_dir,
            thumbnails_dir,
            favicons_dir,
        })
    }
}

/// Move a single file to `new` when the source exists and the destination does
/// not. Best-effort, like `migrate_dir`.
fn migrate_file(old: &Path, new: &Path) {
    if old == new || !old.is_file() || new.exists() {
        return;
    }
    if let Err(e) = std::fs::rename(old, new) {
        tracing::warn!("app_dirs: cannot migrate {:?} -> {:?}: {e}", old, new);
    }
}

/// Move the contents of `old` into `new`, skipping entries that already exist
/// at the destination, then remove `old` if it is left empty. Best-effort: a
/// failure only costs regeneration of cache data, so it is logged, not fatal.
fn migrate_dir(old: &Path, new: &Path) {
    if !old.is_dir() || old == new {
        return;
    }
    let entries = match std::fs::read_dir(old) {
        Ok(entries) => entries,
        Err(e) => {
            tracing::warn!("app_dirs: cannot read {:?}: {e}", old);
            return;
        }
    };
    for entry in entries.filter_map(|e| e.ok()) {
        let dest = new.join(entry.file_name());
        if dest.exists() {
            continue;
        }
        if let Err(e) = std::fs::rename(entry.path(), &dest) {
            tracing::warn!(
                "app_dirs: cannot migrate {:?} -> {:?}: {e}",
                entry.path(),
                dest
            );
        }
    }
    let is_empty = std::fs::read_dir(old)
        .map(|mut i| i.next().is_none())
        .unwrap_or(false);
    if !is_empty {
        return;
    }
    if let Err(e) = std::fs::remove_dir(old) {
        tracing::warn!("app_dirs: cannot remove emptied {:?}: {e}", old);
    }
}
