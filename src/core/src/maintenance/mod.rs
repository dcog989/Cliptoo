//! Data lifecycle maintenance tasks.
//!
//! All functions take a `&DbPool` so they share the single WAL connection.
//! Scheduled tasks run on a Tokio background task; manual tasks are triggered
//! by the UI via `on_maintenance_action`.

use anyhow::Result;
use rusqlite::Connection;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tracing::{info, warn};

use crate::db::{DbPool, schema};

mod cache;
mod deadhead;
mod reclassify;
mod retention;

pub use cache::prune_cache;
pub use deadhead::{deadhead_collect, delete_deadheads, mark_deadheads};
pub use reclassify::reclassify_all;
pub use retention::{RetentionConfig, clear_history, retention};

/// Longest wait before the first scheduled pass. A short session still gets
/// one; a long-running daemon waits only this long once.
const FIRST_PASS_DELAY_SECS: u64 = 5 * 60;

// ── Public API ────────────────────────────────────────────────────────────────

/// Compact the database file in place. Retention deletes leave pages on the
/// freelist, which keeps the file at its high-water mark; `VACUUM` rewrites it
/// and hands that space back to the filesystem. The rewrite lands in the WAL,
/// so it is truncated afterwards or the sidecar keeps the old database size.
pub fn compact(conn: &Connection) -> Result<()> {
    conn.execute_batch(&format!(
        "{}\n{}",
        schema::VACUUM,
        schema::WAL_CHECKPOINT_TRUNCATE
    ))?;
    Ok(())
}

/// Run the full scheduled maintenance cycle:
///   1. Retention (count + age)
///   2. Deadhead (missing file paths)
///   3. Cache pruning (orphaned thumbnails, favicons, full-size images)
///   4. Index housekeeping (planner stats, FTS integrity-check + optimize)
///   5. WAL truncate, so deleted payloads do not linger in the sidecar
pub async fn run_scheduled(
    db: &Arc<DbPool>,
    config: RetentionConfig,
    thumbnails_dir: &Path,
    favicons_dir: &Path,
    images_dir: &Path,
) -> Result<()> {
    let deleted_retention = db.with(|conn| retention(conn, &config)).await?;
    let marked_deadhead = mark_deadheads(db).await?;
    let pruned_cache = prune_cache(db, thumbnails_dir, favicons_dir, images_dir).await?;

    // Same lock acquisition for index housekeeping. `integrity-check` runs
    // first so it inspects the index the pass left behind, not the freshly
    // merged one. It only logs: a divergence means some write bypassed the FTS
    // triggers, and silently rebuilding would hide that. The final checkpoint
    // truncates the WAL: retention just deleted rows, and `secure_delete`
    // zeroes their pages but the old WAL frames would keep the payloads.
    db.with(|conn| {
        if let Err(e) = conn.execute_batch(schema::FTS_INTEGRITY_CHECK) {
            warn!("maintenance: clips_fts integrity-check failed: {e}");
        }
        conn.execute_batch(schema::PRAGMA_OPTIMIZE)?;
        conn.execute_batch(schema::FTS_OPTIMIZE)?;
        conn.execute_batch(schema::WAL_CHECKPOINT_TRUNCATE)?;
        Ok(())
    })
    .await?;

    info!(
        "maintenance: retention={deleted_retention} deadhead_marked={marked_deadhead} cache_pruned={pruned_cache}"
    );
    Ok(())
}

/// Quiet period after a retention change before the next pass starts, so
/// holding the settings stepper does not run a full maintenance cycle per
/// click.
const RETENTION_DEBOUNCE: std::time::Duration = std::time::Duration::from_millis(800);

/// Spawn the background maintenance task. Runs every `interval` seconds.
/// Retention parameters are read from `retention_rx` at each pass, and a change
/// published by the settings UI starts a pass after a short quiet period
/// instead of waiting for the whole interval.
pub fn spawn_scheduler(
    db: Arc<DbPool>,
    thumbnails_dir: PathBuf,
    favicons_dir: PathBuf,
    images_dir: PathBuf,
    mut retention_rx: tokio::sync::watch::Receiver<RetentionConfig>,
    interval_secs: u64,
) {
    tokio::spawn(async move {
        // First run waits only a few minutes, not the whole interval: a user
        // who runs the app for shorter sessions than the interval would
        // otherwise never prune. Bounded by `interval_secs` so tests (and any
        // short interval) still get a first pass promptly.
        let first_delay = interval_secs.min(FIRST_PASS_DELAY_SECS);
        if !wait_for_next_pass(&mut retention_rx, first_delay).await {
            return;
        }
        loop {
            let cfg = retention_rx.borrow().clone();
            if let Err(e) =
                run_scheduled(&db, cfg, &thumbnails_dir, &favicons_dir, &images_dir).await
            {
                warn!("scheduled maintenance error: {e}");
            }
            if !wait_for_next_pass(&mut retention_rx, interval_secs).await {
                return;
            }
        }
    });
}

/// Wait for the next scheduled pass, or for a retention change plus its quiet
/// period. Returns false when the settings sender was dropped (shutdown).
async fn wait_for_next_pass(
    rx: &mut tokio::sync::watch::Receiver<RetentionConfig>,
    delay_secs: u64,
) -> bool {
    tokio::select! {
        _ = tokio::time::sleep(std::time::Duration::from_secs(delay_secs)) => return true,
        changed = rx.changed() => {
            if changed.is_err() {
                return false;
            }
        }
    }

    // Restart the quiet period on every further change, so a held stepper
    // yields one pass after the user stops clicking.
    loop {
        match tokio::time::timeout(RETENTION_DEBOUNCE, rx.changed()).await {
            Err(_) => return true,
            Ok(Err(_)) => return false,
            Ok(Ok(())) => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    fn config(max_clips: u32) -> RetentionConfig {
        RetentionConfig {
            max_clips,
            max_age_days: 30,
        }
    }

    #[tokio::test]
    async fn retention_change_wakes_the_scheduler_early() {
        let (tx, mut rx) = tokio::sync::watch::channel(config(1000));
        let sender = tx.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(10)).await;
            let _ = sender.send(config(100));
        });
        // Keep the original sender alive: a closed channel means shutdown and
        // would abort the quiet period the test is waiting for.
        let _tx = tx;

        let started = Instant::now();
        assert!(wait_for_next_pass(&mut rx, 3600).await);
        assert!(
            started.elapsed() < Duration::from_secs(10),
            "a retention change must not wait for the full interval"
        );
    }

    #[tokio::test]
    async fn scheduler_stops_when_the_settings_sender_is_dropped() {
        let (tx, mut rx) = tokio::sync::watch::channel(config(1000));
        drop(tx);
        assert!(!wait_for_next_pass(&mut rx, 3600).await);
    }
}
