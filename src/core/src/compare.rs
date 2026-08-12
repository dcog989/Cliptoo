// Compare tool integration.
// See PORTING.md §12 for the temp-file workflow.

use anyhow::{Result, bail};
use std::path::{Path, PathBuf};

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

    let extra_dirs: &[&str] = &["/usr/bin", "/usr/local/bin", "/run/host/usr/bin"];

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

    crate::temp::launch_with_temp_files(
        &[
            ("cliptoo_compare_left_{}.txt", left_content.as_bytes()),
            ("cliptoo_compare_right_{}.txt", right_content.as_bytes()),
        ],
        |paths| {
            let mut cmd = std::process::Command::new(&tool);
            for arg in &extra_args {
                cmd.arg(arg);
            }
            cmd.arg(&paths[0]).arg(&paths[1]);
            cmd.spawn()
                .map(|_| ())
                .map_err(|e| anyhow::anyhow!("spawn diff tool: {e}"))
        },
    )
    .await
}
