use chrono::Local;
use chrono::Utc;

/// Returns the current local time formatted as `[YYYY-MM-DD HH:MM:SS]`.
/// Used by the `timestamp` text transformation.
pub fn local_timestamp() -> String {
    format!("[{}]", Local::now().format("%Y-%m-%d %H:%M:%S"))
}

/// Returns the current UTC time as an ISO 8601 string.
/// Used by maintenance to record the last-cleanup time.
pub fn utc_now_iso() -> String {
    Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string()
}
