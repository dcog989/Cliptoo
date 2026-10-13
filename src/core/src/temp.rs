//! Temp-file helpers shared by the external-tool integrations.
//!
//! Both "compare with a diff tool" (`compare.rs`) and "send to app"
//! (`send_to.rs`) write clip content to temp files, launch an external process
//! that reads them, then delete those files a short while after launch. Keeping
//! that lifecycle here avoids duplicating it. The KWin tracker script
//! (`src/ui/src/source_app.rs`) also stages its script through [`runtime_dir`].

use anyhow::{Context, Result};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use tokio::io::AsyncWriteExt;
use uuid::Uuid;

/// Delay before background temp-file cleanup after a successful launch.
const CLEANUP_DELAY_SECS: u64 = 5;

/// Age at which a file in the runtime directory counts as debris from a crashed
/// run rather than in-flight work: a launched tool holds its file for at most
/// [`CLEANUP_DELAY_SECS`], so anything this old was orphaned.
const STALE_AGE_SECS: u64 = 3600;

/// Ephemeral, per-user directory for files handed to external processes.
///
/// `$XDG_RUNTIME_DIR` is the sanctioned location — a `0700` tmpfs the session
/// clears at logout. `$TMPDIR` is the fallback for sessions that have none.
pub fn runtime_dir() -> PathBuf {
    std::env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .unwrap_or_else(std::env::temp_dir)
        .join("cliptoo")
}

/// Create [`runtime_dir`] with `0700`, drop debris left by a previous crashed
/// run, and return it.
///
/// The mode matters: file contents are clipboard payloads, and the `$TMPDIR`
/// fallback can land in a directory other local users can list.
pub async fn ensure_runtime_dir() -> Result<PathBuf> {
    let dir = runtime_dir();
    tokio::fs::create_dir_all(&dir)
        .await
        .with_context(|| format!("create temp dir {}", dir.display()))?;

    let mode = std::fs::Permissions::from_mode(0o700);
    if let Err(e) = tokio::fs::set_permissions(&dir, mode).await {
        tracing::warn!("temp dir {:?}: cannot set mode 0700: {e}", dir);
    }

    sweep_stale(&dir).await;
    Ok(dir)
}

/// Best-effort removal of files older than [`STALE_AGE_SECS`]. A file still
/// within its cleanup delay may be in the middle of being read by a launched
/// tool, so only clearly orphaned files are touched.
async fn sweep_stale(dir: &Path) {
    let Ok(mut entries) = tokio::fs::read_dir(dir).await else {
        return;
    };
    while let Ok(Some(entry)) = entries.next_entry().await {
        let Ok(meta) = entry.metadata().await else {
            continue;
        };
        let Ok(modified) = meta.modified() else {
            continue;
        };
        if modified
            .elapsed()
            .is_ok_and(|age| age.as_secs() > STALE_AGE_SECS)
        {
            let _ = tokio::fs::remove_file(entry.path()).await;
        }
    }
}

/// Write `content` to `path`, creating the file readable by its owner only.
async fn write_private(path: &Path, content: &[u8]) -> std::io::Result<()> {
    let mut file = tokio::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(path)
        .await?;
    file.write_all(content).await?;
    file.flush().await
}

/// Launch `spawn` with one or more temp files, then manage their lifecycle.
///
/// Each `files` entry maps a filename template to its content. The template
/// must contain `{}`, which is replaced with a shared UUID so every file of one
/// invocation forms a recognizable set, e.g. `"cliptoo_compare_left_{}.txt"`.
///
/// `spawn` receives the fully-resolved paths and is expected to launch the
/// external process. On success the files are removed ~5 s later by a
/// background task; on error they are removed immediately.
pub(crate) async fn launch_with_temp_files<F>(files: &[(&str, &[u8])], spawn: F) -> Result<()>
where
    F: FnOnce(&[PathBuf]) -> Result<()>,
{
    let tmp_dir = ensure_runtime_dir().await?;

    let uid = Uuid::new_v4().to_string();
    let paths: Vec<PathBuf> = files
        .iter()
        .map(|(tmpl, _)| tmp_dir.join(tmpl.replace("{}", &uid)))
        .collect();

    for ((tmpl, content), path) in files.iter().zip(&paths) {
        if let Err(e) = write_private(path, content).await {
            cleanup(&paths).await;
            return Err(e).with_context(|| format!("write temp file {tmpl}"));
        }
    }

    match spawn(&paths) {
        Ok(()) => {
            let cleanup_paths = paths.clone();
            tokio::spawn(async move {
                tokio::time::sleep(std::time::Duration::from_secs(CLEANUP_DELAY_SECS)).await;
                cleanup(&cleanup_paths).await;
            });
            Ok(())
        }
        Err(e) => {
            cleanup(&paths).await;
            Err(e)
        }
    }
}

async fn cleanup(paths: &[PathBuf]) {
    for p in paths {
        let _ = tokio::fs::remove_file(p).await;
    }
}
