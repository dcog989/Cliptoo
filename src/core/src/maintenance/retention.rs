//! Retention and history clearing: the destructive pruning done on a schedule
//! or on demand.

use anyhow::Result;
use rusqlite::{Connection, params};
use tracing::info;

/// Parameters for the scheduled retention pass.
#[derive(Clone)]
pub struct RetentionConfig {
    pub max_clips: u32,
    pub max_age_days: u32,
}

/// Delete clips that exceed `max_clips` (oldest first) or are older than
/// `max_age_days`. Bookmarks and pinned clips (`bump_to_bottom`, `IsPinned`)
/// are exempt — both are deliberate keep signals. Returns the total rows
/// deleted.
pub fn retention(conn: &Connection, cfg: &RetentionConfig) -> Result<u64> {
    let mut deleted: u64 = 0;

    // Age-based: delete clips older than max_age_days, non-bookmarked and not
    // pinned.
    if cfg.max_age_days > 0 {
        let n = conn.execute(
            "DELETE FROM clips
             WHERE IsBookmarked = 0
               AND IsPinned = 0
               AND Timestamp < datetime('now', ?1)",
            params![format!("-{} days", cfg.max_age_days)],
        )? as u64;
        deleted += n;
        if n > 0 {
            info!(
                "retention: removed {n} clips older than {} days",
                cfg.max_age_days
            );
        }
    }

    // Count-based: keep only the most recent max_clips non-bookmarked,
    // non-pinned clips. Pinned clips are exempt, so they are excluded from the
    // keep set too and cannot consume its slots.
    //
    // The `IN (SELECT …)` list is uncorrelated, so SQLite evaluates it once and
    // probes it through a Bloom filter (visible in `EXPLAIN QUERY PLAN`) rather
    // than re-running it per candidate row.
    if cfg.max_clips > 0 {
        let n = conn.execute(
            "DELETE FROM clips
              WHERE IsBookmarked = 0
                AND IsPinned = 0
                AND Id NOT IN (
                    SELECT Id FROM clips
                    WHERE IsBookmarked = 0 AND IsPinned = 0
                    ORDER BY Timestamp DESC
                    LIMIT ?1
                )",
            params![cfg.max_clips],
        )? as u64;
        deleted += n;
        if n > 0 {
            info!(
                "retention: removed {n} clips exceeding max_clips={}",
                cfg.max_clips
            );
        }
    }

    Ok(deleted)
}

/// Delete all non-bookmarked, non-pinned clips. Bookmarks and pins are
/// deliberate keep signals — the same two exemptions retention honours — so
/// they always survive a history clear. Returns rows deleted.
pub fn clear_history(conn: &Connection) -> Result<u64> {
    let n = conn.execute(
        "DELETE FROM clips WHERE IsBookmarked = 0 AND IsPinned = 0",
        [],
    )? as u64;
    info!("clear_history: removed {n} clips");
    Ok(n)
}
