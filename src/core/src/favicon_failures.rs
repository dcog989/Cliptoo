//! Persistent per-clip favicon-fetch failure counts.
//!
//! A clip whose favicon cannot be obtained is retried at most
//! [`MAX_FAVICON_FAILURES`] times ever, so a dead domain costs a handful of
//! requests instead of one per session.
//!
//! The ledger lives *beside* (not inside) the favicon cache directory, because
//! the scheduled `prune_cache` pass deletes unrecognised files it finds in that
//! directory. Counts for clips deleted by retention are dropped by [`prune`].

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// Failed attempts after which a clip's favicon is no longer fetched.
pub const MAX_FAVICON_FAILURES: u32 = 5;
/// File name of the ledger, kept next to the favicon cache directory.
pub const FAVICON_FAILURES_FILE: &str = "favicon_failures.txt";

/// Serialises the read-modify-write of the ledger. Concurrent fetch batches
/// otherwise lose each other's increments.
fn ledger_lock() -> &'static Mutex<()> {
    static LOCK: Mutex<()> = Mutex::new(());
    &LOCK
}

/// Path of the ledger file for a favicon cache directory.
pub fn path(favicons_dir: &Path) -> PathBuf {
    favicons_dir
        .parent()
        .unwrap_or(favicons_dir)
        .join(FAVICON_FAILURES_FILE)
}

/// Read the per-clip failure counts, as `clip_id -> failures`.
pub fn load(favicons_dir: &Path) -> HashMap<i64, u32> {
    let Ok(raw) = std::fs::read_to_string(path(favicons_dir)) else {
        return HashMap::new();
    };
    raw.lines()
        .filter_map(|line| line.split_once(':'))
        .filter_map(|(id, count)| Some((id.trim().parse().ok()?, count.trim().parse().ok()?)))
        .collect()
}

fn save(favicons_dir: &Path, failures: &HashMap<i64, u32>) {
    let mut ids: Vec<_> = failures.keys().copied().collect();
    ids.sort_unstable();
    let mut out = String::new();
    for id in ids {
        out.push_str(&format!("{id}:{}\n", failures[&id]));
    }
    let _ = std::fs::write(path(favicons_dir), out);
}

/// Count one failed fetch for `clip_id`, capped at [`MAX_FAVICON_FAILURES`].
pub fn increment(favicons_dir: &Path, clip_id: i64) {
    let _guard = ledger_lock()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let mut failures = load(favicons_dir);
    let count = failures.entry(clip_id).or_insert(0);
    *count = count.saturating_add(1).min(MAX_FAVICON_FAILURES);
    save(favicons_dir, &failures);
}

/// Clear one clip's failures after a successful fetch.
pub fn reset(favicons_dir: &Path, clip_id: i64) {
    let _guard = ledger_lock()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let mut failures = load(favicons_dir);
    if failures.remove(&clip_id).is_some() {
        save(favicons_dir, &failures);
    }
}

/// Whether `clip_id` has used up its fetch budget.
pub fn exhausted(failures: &HashMap<i64, u32>, clip_id: i64) -> bool {
    failures.get(&clip_id).copied().unwrap_or(0) >= MAX_FAVICON_FAILURES
}

/// Drop counts for clips that no longer exist, so the ledger cannot grow with
/// deleted history.
pub fn prune(favicons_dir: &Path, live_ids: &HashSet<i64>) {
    let _guard = ledger_lock()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let mut failures = load(favicons_dir);
    let before = failures.len();
    failures.retain(|id, _| live_ids.contains(id));
    if failures.len() != before {
        save(favicons_dir, &failures);
    }
}

/// Forget every count. Called by the "Clear caches" maintenance action so a
/// cache clear also re-allows favicon retries.
pub fn reset_all(favicons_dir: &Path) {
    let _ = std::fs::remove_file(path(favicons_dir));
}

#[cfg(test)]
mod tests {
    use super::{MAX_FAVICON_FAILURES, exhausted, increment, load, prune, reset, reset_all};
    use std::collections::HashSet;

    fn temp_dir() -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "cliptoo-favicon-failures-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(dir.join("favicons")).unwrap();
        dir
    }

    #[test]
    fn budget_exhausts_at_limit_and_resets_on_clear() {
        let dir = temp_dir();
        let fav_dir = dir.join("favicons");

        assert!(!exhausted(&load(&fav_dir), 42));
        for _ in 0..MAX_FAVICON_FAILURES {
            increment(&fav_dir, 42);
        }
        assert!(exhausted(&load(&fav_dir), 42));

        reset_all(&fav_dir);
        assert!(!exhausted(&load(&fav_dir), 42));

        increment(&fav_dir, 7);
        increment(&fav_dir, 7);
        reset(&fav_dir, 7);
        assert!(!exhausted(&load(&fav_dir), 7));

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn prune_drops_counts_for_deleted_clips() {
        let dir = temp_dir();
        let fav_dir = dir.join("favicons");
        increment(&fav_dir, 1);
        increment(&fav_dir, 2);

        prune(&fav_dir, &HashSet::from([1]));
        let failures = load(&fav_dir);
        assert!(failures.contains_key(&1));
        assert!(!failures.contains_key(&2));

        std::fs::remove_dir_all(&dir).unwrap();
    }
}
