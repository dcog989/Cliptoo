use std::path::Path;
use tracing_appender::rolling::{RollingFileAppender, Rotation};
use tracing_subscriber::Layer;
use tracing_subscriber::Registry;
use tracing_subscriber::filter::{EnvFilter, LevelFilter};
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

/// Fixed number of days to keep rotated log files. Not user-configurable.
pub const LOG_RETENTION_DAYS: u32 = 10;

/// Must be kept alive for the entire program lifetime.
pub struct LogGuard {
    _guard: tracing_appender::non_blocking::WorkerGuard,
}

/// Initialise tracing:
///   - stderr with `RUST_LOG` env‑filter (fallback `"info"`)
///   - daily‑rotating file `{logs_dir}/cliptoo.YYYY-MM-DD.log` gated by `level`
///     (use [`latest_log_path`] to resolve the current file)
///
/// Old rotated logs beyond [`LOG_RETENTION_DAYS`] are removed at startup.
pub fn init(logs_dir: &Path, level: LevelFilter) -> LogGuard {
    cleanup_old_logs(logs_dir, LOG_RETENTION_DAYS);

    let file_appender = RollingFileAppender::builder()
        .rotation(Rotation::DAILY)
        .filename_prefix("cliptoo")
        .filename_suffix("log")
        .build(logs_dir)
        .expect("initializing rolling file appender failed");
    let (non_blocking, guard) = tracing_appender::non_blocking(file_appender);

    // If RUST_LOG is set to an invalid value it's silently ignored (can't log
    // before the subscriber is installed). Use e.g. RUST_LOG=cliptoo=debug.
    // Suppress noisy zbus proxy-cache WARN messages from portal sessions.
    let stderr_filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info"))
        .add_directive("zbus::proxy=error".parse().unwrap());

    let stderr_layer = tracing_subscriber::fmt::layer()
        .with_writer(std::io::stderr)
        .with_ansi(true)
        .with_filter(stderr_filter);

    let file_filter =
        EnvFilter::new(level.to_string()).add_directive("zbus::proxy=error".parse().unwrap());

    let file_layer = tracing_subscriber::fmt::layer()
        .with_writer(non_blocking)
        .with_ansi(false)
        .with_filter(file_filter);

    Registry::default()
        .with(stderr_layer)
        .with(file_layer)
        .init();

    LogGuard { _guard: guard }
}

/// Path of the most recent daily log file (largest `YYYY-MM-DD` suffix), if
/// any exists. The date is fixed-width zero-padded, so lexicographic `max`
/// over the file names picks the newest.
pub fn latest_log_path(logs_dir: &Path) -> Option<std::path::PathBuf> {
    std::fs::read_dir(logs_dir)
        .ok()?
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with("cliptoo.") && n.ends_with(".log"))
        })
        .max()
}

fn cleanup_old_logs(logs_dir: &Path, retention_days: u32) {
    let now = std::time::SystemTime::now();
    let max_age = std::time::Duration::from_secs(retention_days as u64 * 86400);

    let Ok(entries) = std::fs::read_dir(logs_dir) else {
        return;
    };

    for entry in entries.flatten() {
        let path = entry.path();
        let name = match path.file_name().and_then(|n| n.to_str()) {
            Some(n) => n,
            None => continue,
        };

        // The DAILY appender never writes a bare `cliptoo.log`, so exclude it
        // defensively to avoid deleting a user-placed file with that name.
        if name == "cliptoo.log" {
            continue;
        }
        // Owned files: current `cliptoo.2026-08-01.log` scheme, plus legacy
        // `cliptoo-latest*` generations from before the timestamp moved before
        // the extension (pruned on first run).
        let is_current = name.starts_with("cliptoo.") && name.ends_with(".log");
        let is_legacy = name.starts_with("cliptoo-latest");
        if !is_current && !is_legacy {
            continue;
        }

        let modified = match std::fs::metadata(&path).and_then(|m| m.modified()) {
            Ok(t) => t,
            Err(_) => continue,
        };

        if now.duration_since(modified).is_ok_and(|age| age > max_age) {
            let _ = std::fs::remove_file(&path);
        }
    }
}
