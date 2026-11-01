//! Deadhead detection: file-path clips whose path no longer exists on disk.
//! They can be flagged (strikethrough in the UI) or deleted.

use anyhow::Result;
use rusqlite::{Connection, params};
use std::path::Path;
use std::sync::Arc;
use tracing::{info, warn};

use crate::db::DbPool;
use crate::db::models::ClipType;

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

/// Collect file-type clip ids, stored content and current `IsDeadhead` flag
/// for deadhead processing. Split from the filesystem checks so those run
/// outside `db.with`, and the flag lets a later pass skip rows whose state
/// would not change.
///
/// `unbookmarked_only` is set by the delete path: bookmarks are a deliberate
/// keep signal (retention and `clear_history` spare them too), and a path on a
/// temporarily unmounted drive is indistinguishable from one that is gone.
pub fn deadhead_collect(
    conn: &Connection,
    unbookmarked_only: bool,
) -> Result<Vec<(i64, String, bool)>> {
    let bookmark_filter = if unbookmarked_only {
        " AND IsBookmarked = 0"
    } else {
        ""
    };
    let mut stmt = conn.prepare_cached(&format!(
        "SELECT Id, Content, IsDeadhead FROM clips
         WHERE ClipType IN ({types}){bookmark_filter}",
        types = deadhead_clip_types(),
    ))?;
    let mut out = Vec::new();
    for r in stmt.query_map([], |row| {
        Ok((row.get(0)?, row.get(1)?, row.get::<_, i32>(2)? != 0))
    })? {
        match r {
            Ok(pair) => out.push(pair),
            Err(e) => warn!("deadhead: row read error: {e}"),
        }
    }
    Ok(out)
}

/// True when any path in a file clip's `Content` still exists or cannot be
/// checked. Single-path clips store one path; multi-selection copies store one
/// path per line, so a clip is only a deadhead once every referenced path is
/// gone.
///
/// `try_exists` reports "cannot tell" (permission denied, I/O error) as `Err`,
/// which is treated as alive: deleting a clip because its directory is
/// unreadable or its drive is briefly absent loses data the user still has.
fn clip_paths_exist(content: &str) -> bool {
    content
        .lines()
        .filter(|l| !l.trim().is_empty())
        .any(|l| Path::new(l).try_exists().unwrap_or(true))
}

/// Split collected rows into (still missing, path is back) using the
/// filesystem. Runs on a blocking thread: a path on an unresponsive network
/// mount can block the check for a long time.
fn sort_by_existence(rows: Vec<(i64, String, bool)>) -> (Vec<(i64, String)>, Vec<i64>) {
    let mut gone = Vec::new();
    let mut back = Vec::new();
    for (id, path_str, was_dead) in rows {
        if clip_paths_exist(&path_str) {
            // Only a row flagged in an earlier pass needs a clearing write.
            if was_dead {
                back.push(id);
            }
        } else {
            gone.push((id, path_str));
        }
    }
    (gone, back)
}

/// Mark file-type clips whose path no longer exists as deadheads
/// (`IsDeadhead = 1`) without deleting them. The UI shows these with
/// strikethrough. Clears the flag for paths that have come back. Returns the
/// number of rows newly marked.
///
/// The DB mutex is held only for the initial collect and for one final
/// batched-transaction update. Only rows whose flag actually changes get a
/// write, so a pass over a large but healthy history updates nothing.
pub async fn mark_deadheads(db: &Arc<DbPool>) -> Result<u64> {
    let rows = db.with(|conn| deadhead_collect(conn, false)).await?;
    let (gone, back) = tokio::task::spawn_blocking(move || sort_by_existence(rows)).await?;

    if gone.is_empty() && back.is_empty() {
        return Ok(0);
    }

    let newly_marked = db
        .with(move |conn| {
            let tx = conn.unchecked_transaction()?;
            let mut newly_marked: u64 = 0;
            for (id, path_str) in &gone {
                // `AND IsDeadhead = 0` so a re-copy during the pass (which
                // clears the flag) is not marked dead again by a stale check.
                let n = tx.execute(
                    "UPDATE clips SET IsDeadhead = 1 WHERE Id = ?1 AND IsDeadhead = 0",
                    params![id],
                )?;
                if n > 0 {
                    newly_marked += 1;
                    info!("deadhead: marked clip {id} — path gone: {path_str}");
                }
            }
            // Path is back — clear the flag in case it was previously marked.
            for id in &back {
                tx.execute(
                    "UPDATE clips SET IsDeadhead = 0 WHERE Id = ?1 AND IsDeadhead = 1",
                    params![id],
                )?;
            }
            tx.commit()?;
            Ok(newly_marked)
        })
        .await?;

    Ok(newly_marked)
}

/// Delete DB records for file-path clips whose path no longer exists on disk.
/// Bookmarked clips are spared (see [`deadhead_collect`]). Returns the number
/// of rows deleted.
///
/// The DB mutex is held only for the initial collect and for one final
/// batched-transaction delete. The filesystem checks run on a blocking thread
/// so a slow mount cannot stall the runtime.
pub async fn delete_deadheads(db: &Arc<DbPool>) -> Result<u64> {
    let rows = db.with(|conn| deadhead_collect(conn, true)).await?;
    let gone = tokio::task::spawn_blocking(move || {
        rows.into_iter()
            .filter(|(_, path_str, _)| !clip_paths_exist(path_str))
            .map(|(id, path_str, _)| (id, path_str))
            .collect::<Vec<_>>()
    })
    .await?;

    if gone.is_empty() {
        return Ok(0);
    }

    let deleted = gone.len() as u64;
    db.with(move |conn| {
        let tx = conn.unchecked_transaction()?;
        // Ids are integers built here, never user text; one statement instead
        // of a round-trip per clip.
        let ids = gone
            .iter()
            .map(|(id, _)| id.to_string())
            .collect::<Vec<_>>()
            .join(",");
        tx.execute(&format!("DELETE FROM clips WHERE Id IN ({ids})"), [])?;
        for (id, path_str) in &gone {
            info!("deadhead: removed clip {id} — path gone: {path_str}");
        }
        tx.commit()?;
        Ok(())
    })
    .await?;

    Ok(deleted)
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
             'file_dev', 'file_danger', 'file_data', 'file_text', 'file_generic', 'folder'"
        );
    }
}
