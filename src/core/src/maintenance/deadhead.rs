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
/// The DB mutex is held only for the initial collect and for one final
/// batched-transaction update — `Path::exists` syscalls in between run with
/// no lock held. Previously every row's update acquired the lock and ran as
/// its own autocommitted statement; for a large history that was one lock
/// acquisition per clip instead of one for the whole pass.
pub async fn mark_deadheads(db: &Arc<DbPool>) -> Result<u64> {
    let rows = db.with(deadhead_collect).await?;

    // Filesystem checks — no DB lock held.
    let mut gone: Vec<(i64, String)> = Vec::new();
    let mut back: Vec<i64> = Vec::new();
    for (id, path_str) in rows {
        if clip_paths_exist(&path_str) {
            back.push(id);
        } else {
            gone.push((id, path_str));
        }
    }

    if gone.is_empty() && back.is_empty() {
        return Ok(0);
    }

    let newly_marked = db
        .with(move |conn| {
            let tx = conn.unchecked_transaction()?;
            let mut newly_marked: u64 = 0;
            for (id, path_str) in &gone {
                // `AND IsDeadhead = 0` so rows flagged in a previous pass are
                // not counted (or logged) again — the return value is the
                // number of rows newly marked.
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

/// Delete a single clip by id. Used by the async deadhead driver.
fn deadhead_delete(conn: &Connection, id: i64) -> Result<()> {
    conn.execute("DELETE FROM clips WHERE Id = ?1", params![id])?;
    Ok(())
}

/// Delete DB records for file-path clips whose path no longer exists on disk.
/// Returns the number of rows deleted.
///
/// The DB mutex is held only for the initial collect and for one final
/// batched-transaction delete — the `Path::exists` syscalls in between run
/// with no lock held, so a large file history does not stall the clipboard
/// listener or search while the manual "Deadhead" action runs.
pub async fn delete_deadheads(db: &Arc<DbPool>) -> Result<u64> {
    let rows = db.with(deadhead_collect).await?;

    // Filesystem checks — no DB lock held.
    let mut gone: Vec<(i64, String)> = Vec::new();
    for (id, path_str) in rows {
        if !clip_paths_exist(&path_str) {
            gone.push((id, path_str));
        }
    }

    if gone.is_empty() {
        return Ok(0);
    }

    let deleted = gone.len() as u64;
    db.with(move |conn| {
        let tx = conn.unchecked_transaction()?;
        for (id, path_str) in &gone {
            deadhead_delete(&tx, *id)?;
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
