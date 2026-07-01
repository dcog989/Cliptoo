// Compare tool integration.
// See PORTING.md §12 for the temp-file workflow.

use anyhow::{Context, Result, bail};
use std::path::{Path, PathBuf};
use uuid::Uuid;

/// Candidates searched in order when `compare_tool_path` is empty.
const TOOL_CANDIDATES: &[(&str, &[&str])] = &[
    ("meld", &[]),
    ("kompare", &[]),
    ("code", &["--diff"]),
    ("kdiff3", &[]),
    ("diffuse", &[]),
];

/// Discover the diff tool binary, searching PATH then common install locations.
fn discover_tool(explicit_path: &str) -> Result<(PathBuf, Vec<String>)> {
    if !explicit_path.is_empty() {
        let p = PathBuf::from(explicit_path);
        if p.is_file() {
            return Ok((p, vec![]));
        }
        bail!("compare_tool_path '{}' is not a file", explicit_path);
    }

    let extra_dirs: &[&str] = &[
        "/usr/bin",
        "/usr/local/bin",
        "/var/lib/flatpak/exports/bin",
        "/run/host/usr/bin",
    ];

    for (name, extra_args) in TOOL_CANDIDATES {
        // Check PATH first.
        if let Ok(path) = which::which(name) {
            return Ok((path, extra_args.iter().map(|s| s.to_string()).collect()));
        }
        // Fallback to known fixed locations.
        for dir in extra_dirs {
            let candidate = Path::new(dir).join(name);
            if candidate.is_file() {
                return Ok((
                    candidate,
                    extra_args.iter().map(|s| s.to_string()).collect(),
                ));
            }
        }
    }

    bail!("no diff tool found; set compare_tool_path in settings")
}

/// Write two clips to temp files and launch the configured diff tool.
///
/// The tool is launched without waiting; temp files are deleted ~5 s after
/// launch (or immediately on error).
pub async fn compare_clips(
    left_content: &str,
    right_content: &str,
    compare_tool_path: &str,
) -> Result<()> {
    let (tool, extra_args) = discover_tool(compare_tool_path)?;

    let tmp_dir = std::env::temp_dir().join("Cliptoo");
    tokio::fs::create_dir_all(&tmp_dir)
        .await
        .context("create Cliptoo tmp dir")?;

    let uid = Uuid::new_v4().to_string();
    let left_path = tmp_dir.join(format!("cliptoo_compare_left_{uid}.txt"));
    let right_path = tmp_dir.join(format!("cliptoo_compare_right_{uid}.txt"));

    let write_result = async {
        tokio::fs::write(&left_path, left_content.as_bytes())
            .await
            .context("write left temp file")?;
        tokio::fs::write(&right_path, right_content.as_bytes())
            .await
            .context("write right temp file")?;
        Ok::<(), anyhow::Error>(())
    }
    .await;

    if let Err(e) = write_result {
        let _ = tokio::fs::remove_file(&left_path).await;
        let _ = tokio::fs::remove_file(&right_path).await;
        return Err(e);
    }

    let mut cmd = std::process::Command::new(&tool);
    for arg in &extra_args {
        cmd.arg(arg);
    }
    cmd.arg(&left_path).arg(&right_path);

    match cmd.spawn() {
        Ok(_) => {
            // Background cleanup after 5 s.
            let lp = left_path.clone();
            let rp = right_path.clone();
            tokio::spawn(async move {
                tokio::time::sleep(std::time::Duration::from_secs(5)).await;
                let _ = tokio::fs::remove_file(&lp).await;
                let _ = tokio::fs::remove_file(&rp).await;
            });
            Ok(())
        }
        Err(e) => {
            let _ = tokio::fs::remove_file(&left_path).await;
            let _ = tokio::fs::remove_file(&right_path).await;
            Err(anyhow::anyhow!("spawn diff tool: {e}"))
        }
    }
}
