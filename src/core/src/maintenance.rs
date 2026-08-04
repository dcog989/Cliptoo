//! Data lifecycle maintenance tasks.
//!
//! All functions take a `&DbPool` so they share the single WAL connection.
//! Scheduled tasks run on a Tokio background task; manual tasks are triggered
//! by the UI via `on_maintenance_action`.

use anyhow::Result;
use rusqlite::{Connection, params};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tracing::{info, warn};

use crate::content::classifier::ContentProcessor;
use crate::content::hash::normalize_line_endings;
use crate::db::DbPool;
use crate::db::models::ClipType;
use crate::stats;
use crate::time::utc_now_iso;

use crate::image::HASH_FILENAME_PREFIX_LEN;

// ── Public API ────────────────────────────────────────────────────────────────

/// Parameters for the scheduled retention pass.
pub struct RetentionConfig {
    pub max_clips: u32,
    pub max_age_days: u32,
}

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

/// Delete non-bookmarked clips that exceed `max_clips` (oldest first) or are
/// older than `max_age_days`. Returns the total number of rows deleted.
pub fn retention(conn: &Connection, cfg: &RetentionConfig) -> Result<u64> {
    let mut deleted: u64 = 0;

    // Age-based: delete clips older than max_age_days, non-bookmarked only.
    if cfg.max_age_days > 0 {
        let n = conn.execute(
            "DELETE FROM clips
             WHERE IsBookmarked = 0
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

    // Count-based: keep only the most recent max_clips non-bookmarked clips.
    if cfg.max_clips > 0 {
        let n = conn.execute(
            "DELETE FROM clips
              WHERE IsBookmarked = 0
                AND Id NOT IN (
                    SELECT Id FROM clips
                    WHERE IsBookmarked = 0
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

/// SQL `IN (...)` list of the clip types deadhead detection applies to,
/// derived from `ClipType::is_file_clip()` so it cannot drift from the model's
/// stored string values.
fn deadhead_clip_types() -> String {
    ClipType::ALL
        .iter()
        .filter(|t| t.is_file_clip())
        .map(|t| format!("'{}'", t.as_str()))
        .collect::<Vec<_>>()
        .join(", ")
}

/// Collect file-type clip ids and paths for deadhead processing.
/// Split from the delete step so filesystem checks happen outside `db.with`.
pub fn deadhead_collect(conn: &Connection) -> Result<Vec<(i64, String)>> {
    let mut stmt = conn.prepare_cached(&format!(
        "SELECT Id, Content FROM clips WHERE ClipType IN ({types})",
        types = deadhead_clip_types(),
    ))?;
    let mut out = Vec::new();
    for r in stmt.query_map([], |row| Ok((row.get(0)?, row.get(1)?)))? {
        match r {
            Ok(pair) => out.push(pair),
            Err(e) => warn!("deadhead: row read error: {e}"),
        }
    }
    Ok(out)
}

/// True when any path in a file clip's `Content` still exists. Single-path
/// clips store one path; multi-selection copies store one path per line, so a
/// clip is only a deadhead once every referenced path is gone.
fn clip_paths_exist(content: &str) -> bool {
    content
        .lines()
        .filter(|l| !l.trim().is_empty())
        .any(|l| Path::new(l).exists())
}

/// Mark file-type clips whose path no longer exists as deadheads
/// (`IsDeadhead = 1`) without deleting them. The UI shows these with
/// strikethrough. Clears the flag for paths that have come back. Returns the
/// number of rows newly marked.
///
/// The DB mutex is released between the collect step and each per-row update
/// so that `Path::exists` syscalls do not hold the connection lock.
pub async fn mark_deadheads(db: &Arc<DbPool>) -> Result<u64> {
    let rows = db.with(deadhead_collect).await?;
    let mut marked: u64 = 0;
    for (id, path_str) in rows {
        let exists = clip_paths_exist(&path_str);
        match db
            .with(move |conn| {
                if !exists {
                    conn.execute("UPDATE clips SET IsDeadhead = 1 WHERE Id = ?1", params![id])?;
                    Ok(true)
                } else {
                    // Path is back — clear the flag in case it was previously marked.
                    conn.execute(
                        "UPDATE clips SET IsDeadhead = 0 WHERE Id = ?1 AND IsDeadhead = 1",
                        params![id],
                    )?;
                    Ok(false)
                }
            })
            .await
        {
            Ok(true) => {
                marked += 1;
                info!("deadhead: marked clip {id} — path gone: {path_str}");
            }
            Ok(false) => {}
            Err(e) => warn!("deadhead: failed to update clip {id}: {e}"),
        }
    }
    Ok(marked)
}

/// Delete a single clip by id. Used by the async deadhead driver.
fn deadhead_delete(conn: &Connection, id: i64) -> Result<()> {
    conn.execute("DELETE FROM clips WHERE Id = ?1", params![id])?;
    Ok(())
}

/// Delete DB records for file-path clips whose path no longer exists on disk.
/// Returns the number of rows deleted.
///
/// NOTE: This sync variant is kept for direct / test use. In the scheduled
/// maintenance path `run_scheduled` calls the async split version instead so
/// that `Path::exists` syscalls do not hold the DB mutex.
pub fn deadhead(conn: &Connection) -> Result<u64> {
    let rows = deadhead_collect(conn)?;
    let mut deleted: u64 = 0;
    for (id, path_str) in rows {
        if !clip_paths_exist(&path_str) {
            match deadhead_delete(conn, id) {
                Ok(_) => {
                    deleted += 1;
                    info!("deadhead: removed clip {id} — path gone: {path_str}");
                }
                Err(e) => warn!("deadhead: failed to delete clip {id}: {e}"),
            }
        }
    }
    Ok(deleted)
}

/// Delete thumbnail and favicon files on disk that have no matching DB record.
/// Returns the total number of files deleted.
pub async fn prune_cache(
    db: &Arc<DbPool>,
    thumbnails_dir: &Path,
    favicons_dir: &Path,
) -> Result<u64> {
    // Collect the 16-char hash prefixes that are still in the DB.
    // Thumbnail filenames use only the first 16 chars of ContentHash, so
    // storing prefixes (rather than full 64-char hashes) lets us use a direct
    // HashSet::contains lookup — O(1) per file instead of O(n).
    let known_prefixes: std::collections::HashSet<String> = db
        .with(|conn| {
            let mut stmt = conn.prepare_cached("SELECT ContentHash FROM clips")?;
            let prefixes = stmt
                .query_map([], |row| row.get::<_, String>(0))?
                .filter_map(|r| r.ok())
                .map(|h| h[..HASH_FILENAME_PREFIX_LEN.min(h.len())].to_string())
                .collect();
            Ok(prefixes)
        })
        .await?;

    let mut pruned: u64 = 0;
    for dir in [thumbnails_dir, favicons_dir] {
        let Ok(entries) = std::fs::read_dir(dir) else {
            continue;
        };
        for entry in entries.filter_map(|e| e.ok()) {
            let fname = entry.file_name();
            let name = fname.to_string_lossy();
            // Thumbnail files are named `{hash[..16]}.webp` (list-cell) or
            // `{hash[..16]}_preview.webp` (preview). Strip the extension, then
            // strip an optional `_preview` suffix to recover the bare 16-char
            // hash prefix before checking DB membership.
            let stem = name.rsplit_once('.').map(|(s, _)| s).unwrap_or(&name);
            let hash_prefix = stem.strip_suffix("_preview").unwrap_or(stem);
            if !known_prefixes.contains(hash_prefix) {
                if let Err(e) = tokio::fs::remove_file(entry.path()).await {
                    warn!("prune_cache: failed to remove {:?}: {e}", entry.path());
                } else {
                    pruned += 1;
                }
            }
        }
    }
    Ok(pruned)
}

/// Delete non-bookmarked clips whose `SizeInBytes` exceeds `threshold_bytes`.
/// Returns the number of rows deleted.
pub fn prune_oversized(conn: &Connection, threshold_bytes: i64) -> Result<u64> {
    let n = conn.execute(
        "DELETE FROM clips WHERE IsBookmarked = 0 AND SizeInBytes > ?1",
        params![threshold_bytes],
    )? as u64;
    if n > 0 {
        info!("prune_oversized: removed {n} clips exceeding {threshold_bytes} bytes");
    }
    Ok(n)
}

/// Delete all non-bookmarked clips (and optionally bookmarked ones too).
/// Returns the number of rows deleted.
pub fn clear_history(conn: &Connection, include_bookmarked: bool) -> Result<u64> {
    let n = if include_bookmarked {
        conn.execute("DELETE FROM clips", [])? as u64
    } else {
        conn.execute("DELETE FROM clips WHERE IsBookmarked = 0", [])? as u64
    };
    info!("clear_history: removed {n} clips (include_bookmarked={include_bookmarked})");
    Ok(n)
}

/// Re-run `ContentProcessor` on every stored clip and correct rows whose
/// `ClipType` no longer matches the classifier's result. Returns the number of
/// rows updated.
///
/// A row is only rewritten when the recomputed `ClipType` differs from the
/// stored one — a genuinely misclassified clip. Correctly classified rows
/// (including freshly copied ones) are left untouched, so reclassify never
/// reports clips that are already right.
///
/// `WasTrimmed` / `HasLeadingWhitespace` are preserved: the stored `Content`
/// is the trimmed payload, so those insert-time facts cannot be re-derived
/// from it (reclassifying trimmed content would always read them as false).
/// The other fields are refreshed only alongside a type change.
pub fn reclassify_all(conn: &Connection) -> Result<u64> {
    // Fetch all ids + raw content + currently stored ClipType in a single pass.
    // IsFileUri tells the classifier whether the clip was a copied file/folder,
    // so path-looking *text* clips reclassify to FilePath while copied files
    // keep their Folder/file_* classification.
    let rows: Vec<(i64, String, String, bool)> = {
        let mut stmt = conn.prepare_cached(
            "SELECT Id, Content, ClipType, IsFileUri FROM clips WHERE Content IS NOT NULL",
        )?;
        let mut out = Vec::new();
        for r in stmt.query_map([], |row| {
            Ok((
                row.get(0)?,
                row.get(1)?,
                row.get(2)?,
                row.get::<_, i32>(3)? != 0,
            ))
        })? {
            match r {
                Ok(row) => out.push(row),
                Err(e) => warn!("reclassify_all: row read error: {e}"),
            }
        }
        out
    };

    let mut updated: u64 = 0;
    for (id, raw, cur_clip_type, is_copied_file) in rows {
        if raw.trim().is_empty() {
            continue;
        }
        let normalised = normalize_line_endings(&raw);
        if let Some(c) = ContentProcessor::process(&normalised, is_copied_file) {
            if c.clip_type.as_str() == cur_clip_type {
                continue;
            }
            conn.execute(
                "UPDATE clips
                 SET ClipType = ?1, PreviewContent = ?2, SizeInBytes = ?3, IsMultiline = ?4
                 WHERE Id = ?5",
                params![
                    c.clip_type.as_str(),
                    c.preview_content,
                    c.size_in_bytes,
                    c.is_multiline as i32,
                    id,
                ],
            )?;
            updated += 1;
        }
    }
    info!("reclassify_all: updated {updated} clips");
    Ok(updated)
}

/// Spawn the background maintenance task. Runs every `interval` seconds.
/// Retention parameters are fixed at spawn time; restart required to pick up
/// settings changes.
pub fn spawn_scheduler(
    db: Arc<DbPool>,
    thumbnails_dir: PathBuf,
    favicons_dir: PathBuf,
    max_clips: u32,
    max_age_days: u32,
    interval_secs: u64,
) {
    tokio::spawn(async move {
        // Stagger first run by one interval so startup isn't burdened.
        tokio::time::sleep(std::time::Duration::from_secs(interval_secs)).await;
        loop {
            let cfg = RetentionConfig {
                max_clips,
                max_age_days,
            };
            if let Err(e) = run_scheduled(&db, cfg, &thumbnails_dir, &favicons_dir).await {
                warn!("scheduled maintenance error: {e}");
            }
            tokio::time::sleep(std::time::Duration::from_secs(interval_secs)).await;
        }
    });
}

#[cfg(test)]
mod tests {
    use super::deadhead_clip_types;

    #[test]
    fn deadhead_list_covers_expected_file_clip_types() {
        // In `ClipType::ALL` declaration order, filtered to file clips only.
        assert_eq!(
            deadhead_clip_types(),
            "'file_image', 'file_video', 'file_audio', 'file_archive', 'file_document', \
             'file_dev', 'file_danger', 'file_text', 'file_generic', 'folder'"
        );
    }
}
