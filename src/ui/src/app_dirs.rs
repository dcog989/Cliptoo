use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

/// Resolved application directory paths.
///
///   config  (~/.config/Cliptoo/)      — settings
///   state   (~/.local/state/Cliptoo/) — logs
///   data    (~/.local/share/Cliptoo/) — clip history DB, full-resolution images
///   cache   (~/.cache/Cliptoo/)       — thumbnails, favicons (regenerable)
#[derive(Clone)]
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
    pub lock_path: PathBuf,
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
        // Single-instance lock. XDG_RUNTIME_DIR is per-user and cleared at
        // logout; the cache dir is only a fallback when it is unset.
        let lock_path = dirs::runtime_dir()
            .unwrap_or_else(|| cache_home.clone())
            .join("cliptoo.lock");

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

        // The history and everything derived from it holds clipboard payloads.
        // XDG does not promise an owner-only home (Debian/Ubuntu default to
        // 0755), so tighten the data-bearing dirs ourselves. Best-effort: a
        // failure must not prevent startup.
        for dir in [&data_dir, &cache_dir, &logs_dir] {
            restrict_to_owner(dir);
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
            &data_dir.join(cliptoo_core::favicon_failures::FAVICON_FAILURES_FILE),
            &cache_dir.join(cliptoo_core::favicon_failures::FAVICON_FAILURES_FILE),
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
            lock_path,
        })
    }
}

/// Set `dir` to owner-only (0700). Best-effort: a directory the user cannot
/// chmod must not stop the app from starting.
fn restrict_to_owner(dir: &Path) {
    use std::os::unix::fs::PermissionsExt;
    if let Err(e) = std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700)) {
        eprintln!("cliptoo: cannot restrict {:?} to mode 0700: {e}", dir);
    }
}

/// Move a single file to `new` when the source exists and the destination does
/// not. Prefers an atomic rename; falls back to copy + rename when the two
/// directories sit on different filesystems (`rename` then fails with `EXDEV`),
/// so non-regenerable files such as the clip DB are not stranded. The copy goes
/// to a temporary name beside the destination and is renamed into place, so a
/// crash mid-copy cannot leave a truncated file that the next start uses.
/// Best-effort, like `migrate_dir`.
fn migrate_file(old: &Path, new: &Path) {
    if old == new || !old.is_file() || new.exists() {
        return;
    }
    if std::fs::rename(old, new).is_ok() {
        return;
    }
    let tmp = temp_sibling(new);
    if let Err(e) = std::fs::copy(old, &tmp) {
        let _ = std::fs::remove_file(&tmp);
        tracing::warn!("app_dirs: cannot migrate {:?} -> {:?}: {e}", old, new);
        return;
    }
    if let Err(e) = std::fs::File::open(&tmp)
        .and_then(|f| f.sync_all())
        .and_then(|()| std::fs::rename(&tmp, new))
    {
        let _ = std::fs::remove_file(&tmp);
        tracing::warn!("app_dirs: cannot move {:?} -> {:?}: {e}", old, new);
        return;
    }
    if let Err(e) = std::fs::remove_file(old) {
        tracing::warn!("app_dirs: copied {:?} but cannot remove source: {e}", old);
    }
}

/// `new` plus a `.migrating` suffix: same directory, so the final rename is
/// atomic.
fn temp_sibling(new: &Path) -> PathBuf {
    let mut name = new.file_name().unwrap_or_default().to_os_string();
    name.push(".migrating");
    new.with_file_name(name)
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
        // Same move as a single file, so a cross-filesystem cache directory
        // (EXDEV) is copied instead of being left behind with a warning.
        migrate_file(&entry.path(), &new.join(entry.file_name()));
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

#[cfg(test)]
mod tests {
    use super::migrate_file;

    fn temp_dir(name: &str) -> std::path::PathBuf {
        let dir =
            std::env::temp_dir().join(format!("cliptoo_app_dirs_{name}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// The clip DB is the one file here that cannot be regenerated: a move must
    /// land the data at the new path, and must never overwrite a live file.
    #[test]
    fn migrate_file_moves_the_source_but_never_overwrites_the_destination() {
        let dir = temp_dir("migrate");
        let old = dir.join("clips.db");
        let new = dir.join("data").join("clips.db");
        std::fs::create_dir_all(new.parent().unwrap()).unwrap();

        std::fs::write(&old, b"history").unwrap();
        migrate_file(&old, &new);
        assert!(!old.exists(), "a moved source is removed");
        assert_eq!(std::fs::read(&new).unwrap(), b"history");

        std::fs::write(&old, b"stale").unwrap();
        std::fs::write(&new, b"live").unwrap();
        migrate_file(&old, &new);
        assert!(old.exists(), "the source stays when the destination exists");
        assert_eq!(std::fs::read(&new).unwrap(), b"live");

        let _ = std::fs::remove_dir_all(&dir);
    }
}
