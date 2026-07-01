use std::path::Path;
use tracing_appender::rolling::{RollingFileAppender, Rotation};
use tracing_subscriber::Layer;
use tracing_subscriber::Registry;
use tracing_subscriber::filter::{EnvFilter, LevelFilter};
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

/// Must be kept alive for the entire program lifetime.
pub struct LogGuard {
    _guard: tracing_appender::non_blocking::WorkerGuard,
}

/// Initialise tracing:
///   - stderr with `RUST_LOG` env‑filter (fallback `"info"`)
///   - daily‑rotating file at `{logs_dir}/cliptoo-latest.log` gated by `level`
///
/// Old rotated logs beyond `retention_days` are removed at startup.
pub fn init(logs_dir: &Path, level: LevelFilter, retention_days: u32) -> LogGuard {
    cleanup_old_logs(logs_dir, retention_days);

    let file_appender = RollingFileAppender::new(Rotation::DAILY, logs_dir, "cliptoo-latest.log");
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

        if !name.starts_with("cliptoo-") || !name.ends_with(".log") {
            continue;
        }
        if name == "cliptoo-latest.log" {
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
