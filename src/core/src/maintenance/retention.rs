//! Retention and history clearing: the destructive pruning done on a schedule
//! or on demand.

use anyhow::Result;
use rusqlite::{Connection, params};
use tracing::info;

use crate::db::queries::EPOCH_TS_PREFIX;

/// Parameters for the scheduled retention pass.
#[derive(Clone)]
pub struct RetentionConfig {
    pub max_clips: u32,
    pub max_age_days: u32,
}

/// SQL predicate matching clips pinned to the bottom via `bump_to_bottom`.
/// Their `Timestamp` is the epoch sentinel, which is not a real time and would
/// otherwise always sort older than any retention cutoff. Retention must treat
/// a manual "move to bottom" like a bookmark — a deliberate keep signal — and
/// never sweep these clips.
fn bottom_pinned_predicate() -> String {
    format!("Timestamp LIKE '{}%'", EPOCH_TS_PREFIX)
}

/// Delete clips that exceed `max_clips` (oldest first) or are older than
/// `max_age_days`. Bookmarks and bottom-pinned clips (`bump_to_bottom`) are
/// exempt — both are deliberate keep signals. Returns the total rows deleted.
pub fn retention(conn: &Connection, cfg: &RetentionConfig) -> Result<u64> {
    let mut deleted: u64 = 0;
    let pinned = bottom_pinned_predicate();

    // Age-based: delete clips older than max_age_days, non-bookmarked and not
    // bottom-pinned.
    if cfg.max_age_days > 0 {
        let n = conn.execute(
            &format!(
                "DELETE FROM clips
                 WHERE IsBookmarked = 0
                   AND NOT ({pinned})
                   AND Timestamp < datetime('now', ?1)"
            ),
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

    // Count-based: keep only the most recent max_clips non-bookmarked clips.
    // Bottom-pinned clips are exempt here too — they sort last, so they would
    // otherwise always be the first cut once the count cap is reached.
    //
    // NOTE: `Id NOT IN (SELECT Id … LIMIT ?1)` re-evaluates the subquery per
    // outer row, so it degrades toward O(n²) once n ≫ max_clips. Fine for a
    // desktop daemon with bounded history; a LEFT JOIN / anti-join rewrite is
    // the fix if scale ever demands it.
    if cfg.max_clips > 0 {
        let n = conn.execute(
            &format!(
                "DELETE FROM clips
                  WHERE IsBookmarked = 0
                    AND NOT ({pinned})
                    AND Id NOT IN (
                        SELECT Id FROM clips
                        WHERE IsBookmarked = 0
                        ORDER BY Timestamp DESC
                        LIMIT ?1
                    )"
            ),
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

/// Delete all non-bookmarked clips. Bookmarked clips are deliberate keep
/// signals, so they always survive a history clear. Returns rows deleted.
pub fn clear_history(conn: &Connection) -> Result<u64> {
    let n = conn.execute("DELETE FROM clips WHERE IsBookmarked = 0", [])? as u64;
    info!("clear_history: removed {n} clips");
    Ok(n)
}
