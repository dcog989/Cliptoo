// "Send To" integration — pipe clip content to a user-defined external app.
//
// Each SendToApp entry (from Settings) has a `name` and `path`. The clip
// content is written to a temp file and the path is passed as the first
// argument, mirroring the behaviour of `code <path>` etc.
//
// The temp file is deleted ~5 s after launch (or immediately on error).

use anyhow::{Context, Result, bail};
use std::path::PathBuf;

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
    crate::temp::launch_with_temp_files(&[("cliptoo_sendto_{}.txt", content.as_bytes())], |paths| {
        std::process::Command::new(exe)
            .arg(&paths[0])
            .spawn()
            .map(|_| ())
            .map_err(|e| anyhow::anyhow!("spawn '{}': {e}", exe.display()))
    })
    .await
}
