//! Data lifecycle maintenance tasks.
//!
//! All functions take a `&DbPool` so they share the single WAL connection.
//! Scheduled tasks run on a Tokio background task; manual tasks are triggered
//! by the UI via `on_maintenance_action`.

use anyhow::Result;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tracing::{info, warn};

use crate::db::DbPool;
use crate::stats;
use crate::time::utc_now_iso;

mod cache;
mod deadhead;
mod reclassify;
mod retention;

pub use cache::prune_cache;
pub use deadhead::{deadhead_collect, delete_deadheads, mark_deadheads};
pub use reclassify::reclassify_all;
pub use retention::{RetentionConfig, clear_history, retention};

// ── Public API ────────────────────────────────────────────────────────────────

/// Run the full scheduled maintenance cycle:
///   1. Retention (count + age)
///   2. Deadhead (missing file paths)
///   3. Cache pruning (orphaned thumbnail/favicon files)
///
/// Updates `LastCleanupTimestamp` on completion.
pub async fn run_scheduled(
    db: &Arc<DbPool>,
    config: RetentionConfig,
    thumbnails_dir: &Path,
    favicons_dir: &Path,
) -> Result<()> {
    let deleted_retention = db.with(|conn| retention(conn, &config)).await?;
    let marked_deadhead = mark_deadheads(db).await?;
    let pruned_cache = prune_cache(db, thumbnails_dir, favicons_dir).await?;

    db.with(|conn| stats::set_stat(conn, stats::KEY_LAST_CLEANUP, &utc_now_iso()))
        .await?;

    info!(
        "maintenance: retention={deleted_retention} deadhead_marked={marked_deadhead} cache_pruned={pruned_cache}"
    );
    Ok(())
}

/// Spawn the background maintenance task. Runs every `interval` seconds.
/// Retention parameters are read from `retention_rx` at each pass, so a change
/// made in the settings UI applies on the next run without a restart.
pub fn spawn_scheduler(
    db: Arc<DbPool>,
    thumbnails_dir: PathBuf,
    favicons_dir: PathBuf,
    retention_rx: tokio::sync::watch::Receiver<RetentionConfig>,
    interval_secs: u64,
) {
    tokio::spawn(async move {
        // Stagger first run by one interval so startup isn't burdened.
        tokio::time::sleep(std::time::Duration::from_secs(interval_secs)).await;
        loop {
            let cfg = retention_rx.borrow().clone();
            if let Err(e) = run_scheduled(&db, cfg, &thumbnails_dir, &favicons_dir).await {
                warn!("scheduled maintenance error: {e}");
            }
            tokio::time::sleep(std::time::Duration::from_secs(interval_secs)).await;
        }
    });
}
