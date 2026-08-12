use std::path::Path;
use std::sync::atomic::{AtomicU8, Ordering};
use tracing_appender::rolling::{RollingFileAppender, Rotation};
use tracing_subscriber::Layer;
use tracing_subscriber::Registry;
use tracing_subscriber::filter::{EnvFilter, LevelFilter, filter_fn};
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

/// Fixed number of days to keep rotated log files. Not user-configurable.
pub const LOG_RETENTION_DAYS: u32 = 10;

// ── Runtime log level ─────────────────────────────────────────────────────────
//
// The on-disk log level is stored in a static so the file layer's filter can
// swap it at runtime without reinstalling the global subscriber. `set_level`
// is called from the Settings "Log level" picker and takes effect immediately.

const LEVEL_OFF: u8 = 0;
const LEVEL_ERROR: u8 = 1;
const LEVEL_WARN: u8 = 2;
const LEVEL_INFO: u8 = 3;
const LEVEL_DEBUG: u8 = 4;
const LEVEL_TRACE: u8 = 5;

static FILE_LEVEL: AtomicU8 = AtomicU8::new(LEVEL_INFO);

fn level_to_u8(level: LevelFilter) -> u8 {
    match level {
        LevelFilter::OFF => LEVEL_OFF,
        LevelFilter::ERROR => LEVEL_ERROR,
        LevelFilter::WARN => LEVEL_WARN,
        LevelFilter::INFO => LEVEL_INFO,
        LevelFilter::DEBUG => LEVEL_DEBUG,
        LevelFilter::TRACE => LEVEL_TRACE,
    }
}

fn u8_to_level(value: u8) -> LevelFilter {
    match value {
        LEVEL_OFF => LevelFilter::OFF,
        LEVEL_ERROR => LevelFilter::ERROR,
        LEVEL_WARN => LevelFilter::WARN,
        LEVEL_INFO => LevelFilter::INFO,
        LEVEL_DEBUG => LevelFilter::DEBUG,
        _ => LevelFilter::TRACE,
    }
}

fn current_level() -> LevelFilter {
    u8_to_level(FILE_LEVEL.load(Ordering::Relaxed))
}

/// Predicate backing the file layer's filter: the live-swappable app level,
/// plus a fixed suppression of noisy zbus proxy-cache WARN messages. Kept as a
/// plain function so the semantics are testable without installing a global
/// subscriber.
fn file_level_allows(target: &str, level: tracing::Level) -> bool {
    if target.starts_with("zbus::proxy") {
        return level <= LevelFilter::ERROR;
    }
    level <= current_level()
}

/// Reconfigure the on-disk log level at runtime. The file layer's filter reads
/// `FILE_LEVEL` on every record, so no subscriber reinstallation is needed.
pub fn set_level(level: LevelFilter) {
    FILE_LEVEL.store(level_to_u8(level), Ordering::Relaxed);
}

/// Must be kept alive for the entire program lifetime.
pub struct LogGuard {
    _guard: tracing_appender::non_blocking::WorkerGuard,
}

/// Initialise tracing:
///   - stderr with `RUST_LOG` env‑filter (fallback `"info"`)
///   - daily‑rotating file `{logs_dir}/cliptoo.YYYY-MM-DD.log` gated by `level`
///     (use [`latest_log_path`] to resolve the current file)
///
/// The file level can be changed later via [`set_level`] (the Settings "Log
/// level" picker) without a restart.
///
/// Old rotated logs beyond [`LOG_RETENTION_DAYS`] are removed at startup.
pub fn init(logs_dir: &Path, level: LevelFilter) -> LogGuard {
    cleanup_old_logs(logs_dir, LOG_RETENTION_DAYS);
    FILE_LEVEL.store(level_to_u8(level), Ordering::Relaxed);

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

    let file_filter = filter_fn(|metadata| file_level_allows(metadata.target(), *metadata.level()));

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn level_u8_round_trips() {
        for level in [
            LevelFilter::OFF,
            LevelFilter::ERROR,
            LevelFilter::WARN,
            LevelFilter::INFO,
            LevelFilter::DEBUG,
            LevelFilter::TRACE,
        ] {
            assert_eq!(u8_to_level(level_to_u8(level)), level);
        }
    }

    #[test]
    fn zbus_proxy_warn_suppressed_at_every_level() {
        // The proxy directive short-circuits before the app level is consulted,
        // so it holds regardless of what other tests set the atomic to.
        assert!(file_level_allows("zbus::proxy", tracing::Level::ERROR));
        assert!(!file_level_allows("zbus::proxy", tracing::Level::WARN));
        assert!(!file_level_allows("zbus::proxy", tracing::Level::INFO));
        assert!(!file_level_allows("zbus::proxy", tracing::Level::DEBUG));
    }

    /// The only test that writes `FILE_LEVEL`; kept single so parallel runs
    /// cannot observe another test's cutoff.
    #[test]
    fn level_cutoff_follows_set_level() {
        set_level(LevelFilter::INFO);
        assert!(file_level_allows("app", tracing::Level::ERROR));
        assert!(file_level_allows("app", tracing::Level::WARN));
        assert!(file_level_allows("app", tracing::Level::INFO));
        assert!(!file_level_allows("app", tracing::Level::DEBUG));
        assert!(!file_level_allows("app", tracing::Level::TRACE));

        set_level(LevelFilter::DEBUG);
        assert!(file_level_allows("app", tracing::Level::DEBUG));
        assert!(!file_level_allows("app", tracing::Level::TRACE));

        set_level(LevelFilter::ERROR);
        assert!(file_level_allows("app", tracing::Level::ERROR));
        assert!(!file_level_allows("app", tracing::Level::WARN));
    }
}
