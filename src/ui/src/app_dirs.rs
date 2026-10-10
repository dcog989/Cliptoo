use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

/// Resolved application directory paths.
///
///   config  (~/.config/Cliptoo/)      — settings
///   state   (~/.local/state/Cliptoo/) — logs
///   data    (~/.local/share/Cliptoo/) — clip history DB, full-resolution images
///   cache   (~/.cache/Cliptoo/)       — thumbnails, favicons (regenerable)
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
        let state_home = dirs::state_dir().context("no state dir (unset HOME?)")?;

        // Config — settings only.
        let config_dir = config_home.join("Cliptoo");
        let settings_path = config_dir.join("settings.json");

        // State — logs (`$XDG_STATE_HOME`, per the Base Directory spec).
        let state_dir = state_home.join("Cliptoo");
        let logs_dir = state_dir.join("Logs");

        // Data — the clip history DB and full-resolution images are referenced
        // by clip rows, so they must survive a cache clear and live under
        // `$XDG_DATA_HOME`.
        let data_dir = data_home.join("Cliptoo");
        let db_path = data_dir.join("clips.db");
        let images_dir = data_dir.join("images");

        // Cache — thumbnails and favicons are derived and rebuilt on demand.
        let cache_dir = cache_home.join("Cliptoo");
        let thumbnails_dir = cache_dir.join("thumbnails");
        let favicons_dir = cache_dir.join("favicons");

        // Create all directories on first run
        for dir in &[
            &config_dir,
            &state_dir,
            &data_dir,
            &cache_dir,
            &logs_dir,
            &images_dir,
            &thumbnails_dir,
            &favicons_dir,
        ] {
            std::fs::create_dir_all(dir)?;
        }

        // One-time migrations from the pre-XDG layout, where the DB, logs and
        // cache all lived under the config dir. Best-effort: a failure leaves
        // data in place rather than discarding it.
        migrate_dir(&config_dir.join("Logs"), &logs_dir);

        // The DB must move with its WAL/SHM sidecars so no uncommitted writes
        // are orphaned. `migrate_file` skips a sidecar whose destination already
        // exists, so an existing DB is never clobbered.
        for suffix in ["", "-wal", "-shm"] {
            migrate_file(
                &config_dir.join(format!("clips.db{suffix}")),
                &data_dir.join(format!("clips.db{suffix}")),
            );
        }

        // Thumbnails/favicons previously lived under the data dir. Move them
        // across so cached favicons (and their failure counters) survive; they
        // regenerate if the move fails.
        migrate_dir(&data_dir.join("thumbnails"), &thumbnails_dir);
        migrate_dir(&data_dir.join("favicons"), &favicons_dir);
        migrate_file(
            &data_dir.join(crate::favicon::FAVICON_FAILURES_FILE),
            &cache_dir.join(crate::favicon::FAVICON_FAILURES_FILE),
        );

        Ok(Self {
            db_path,
            settings_path,
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
/// not. Prefers an atomic rename; falls back to copy + remove when the two
/// directories sit on different filesystems (`rename` then fails with `EXDEV`),
/// so non-regenerable files such as the clip DB are not stranded. Best-effort,
/// like `migrate_dir`.
fn migrate_file(old: &Path, new: &Path) {
    if old == new || !old.is_file() || new.exists() {
        return;
    }
    if std::fs::rename(old, new).is_ok() {
        return;
    }
    match std::fs::copy(old, new) {
        Ok(_) => {
            if let Err(e) = std::fs::remove_file(old) {
                tracing::warn!("app_dirs: copied {:?} but cannot remove source: {e}", old);
            }
        }
        Err(e) => tracing::warn!("app_dirs: cannot migrate {:?} -> {:?}: {e}", old, new),
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
