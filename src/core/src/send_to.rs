// "Send To" integration — pipe clip content to a user-defined external app.
//
// Each SendToApp entry (from Settings) has a `name` and `path`. The clip
// content is written to a temp file and the path is passed as the first
// argument, mirroring the behaviour of `code <path>` etc.
//
// The temp file is deleted ~5 s after launch (or immediately on error).

use anyhow::{Context, Result, bail};
use std::path::PathBuf;
use uuid::Uuid;

/// Launch a user-defined external app with the clip content.
///
/// `app_path` — executable path from `Settings::send_to_apps`.
/// `content`  — full clip content (not preview).
pub async fn send_to(app_path: &str, content: &str) -> Result<()> {
    if app_path.is_empty() {
        bail!("send_to: app_path is empty");
    }

    let exe = PathBuf::from(app_path);
    if exe.is_file() {
        return send_to_exe(&exe, content).await;
    }

    // Try PATH resolution as fallback (allows bare names like "code", "gedit").
    let resolved = which::which(app_path).with_context(|| {
        format!(
            "send_to: '{}' not found on PATH or as absolute path",
            app_path
        )
    })?;
    send_to_exe(&resolved, content).await
}

async fn send_to_exe(exe: &std::path::Path, content: &str) -> Result<()> {
    let tmp_dir = std::env::temp_dir().join("Cliptoo");
    tokio::fs::create_dir_all(&tmp_dir)
        .await
        .context("create Cliptoo tmp dir")?;

    let uid = Uuid::new_v4().to_string();
    let tmp_path = tmp_dir.join(format!("cliptoo_sendto_{uid}.txt"));

    tokio::fs::write(&tmp_path, content.as_bytes())
        .await
        .with_context(|| format!("write send-to temp file {:?}", tmp_path))?;

    match std::process::Command::new(exe).arg(&tmp_path).spawn() {
        Ok(_) => {
            let tp = tmp_path.clone();
            tokio::spawn(async move {
                tokio::time::sleep(std::time::Duration::from_secs(5)).await;
                let _ = tokio::fs::remove_file(&tp).await;
            });
            Ok(())
        }
        Err(e) => {
            let _ = tokio::fs::remove_file(&tmp_path).await;
            Err(anyhow::anyhow!("spawn '{}': {e}", exe.display()))
        }
    }
}
