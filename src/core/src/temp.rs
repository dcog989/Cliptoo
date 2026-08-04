//! Temp-file helpers shared by the external-tool integrations.
//!
//! Both "compare with a diff tool" (`compare.rs`) and "send to app"
//! (`send_to.rs`) write clip content to temp files, launch an external process
//! that reads them, then delete those files a short while after launch. Keeping
//! that lifecycle here avoids duplicating it.

use anyhow::{Context, Result};
use std::path::PathBuf;
use uuid::Uuid;

/// Delay before background temp-file cleanup after a successful launch.
const CLEANUP_DELAY_SECS: u64 = 5;

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
    let tmp_dir = std::env::temp_dir().join("Cliptoo");
    tokio::fs::create_dir_all(&tmp_dir)
        .await
        .context("create Cliptoo tmp dir")?;

    let uid = Uuid::new_v4().to_string();
    let paths: Vec<PathBuf> = files
        .iter()
        .map(|(tmpl, _)| tmp_dir.join(tmpl.replace("{}", &uid)))
        .collect();

    for ((tmpl, content), path) in files.iter().zip(&paths) {
        if let Err(e) = tokio::fs::write(path, content).await {
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
