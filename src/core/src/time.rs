use chrono::Local;

/// Returns the current local time formatted as `[YYYY-MM-DD HH:MM:SS]`.
/// Used by the `timestamp` text transformation.
pub fn local_timestamp() -> String {
    format!("[{}]", Local::now().format("%Y-%m-%d %H:%M:%S"))
}
