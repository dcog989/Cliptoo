//! Reclassification: re-run the content classifier over stored clips and
//! correct rows whose `ClipType` has drifted.

use anyhow::Result;
use rusqlite::{Connection, params};
use std::sync::Arc;
use tracing::{info, warn};

use crate::content::classifier::ContentProcessor;
use crate::content::hash::normalize_line_endings;
use crate::db::DbPool;
use crate::db::models::ClipType;

/// Collect every stored clip row for reclassification.
/// Split from the update step so the classification filesystem checks run
/// outside `db.with` — same shape as `deadhead_collect` for deadhead marking.
fn reclassify_collect(conn: &Connection) -> Result<Vec<(i64, String, String, bool)>> {
    // Fetch all ids + raw content + currently stored ClipType in a single pass.
    // IsFileUri tells the classifier whether the clip was a copied file/folder,
    // so path-looking *text* clips reclassify to FilePath while copied files
    // keep their Folder/file_* classification.
    //
    // `file_image` clips are excluded: their Content is an internal path under
    // `images_dir` (see the listener's image branch), not a user-copied payload.
    // Re-running the classifier on it would misread the path as plain text and
    // reclassify the row from `file_image` to `file_path`, breaking the image
    // preview/thumbnail handlers. Nothing in Content identifies an image clip,
    // so the type is not re-derivable — leave the stored one alone.
    let mut stmt = conn.prepare_cached(
        "SELECT Id, Content, ClipType, IsFileUri FROM clips
         WHERE Content IS NOT NULL AND ClipType != 'file_image'",
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
            Err(e) => warn!("reclassify: row read error: {e}"),
        }
    }
    Ok(out)
}

/// Re-run `ContentProcessor` on every stored clip and correct rows whose
/// `ClipType` no longer matches the classifier's result. Returns the number of
/// rows updated.
///
/// A row is only rewritten when the recomputed `ClipType` differs from the
/// stored one — a genuinely misclassified clip. Correctly classified rows
/// (including freshly copied ones) are left untouched, so reclassify never
/// reports clips that are already right. The one exception: a stored file clip
/// whose path is now missing recomputes as `file_generic` (the classifier sees
/// only current disk state) and is deliberately left with its specific type —
/// deadhead maintenance already flags the missing path.
///
/// `WasTrimmed` / `HasLeadingWhitespace` are preserved: the stored `Content`
/// is the trimmed payload, so those insert-time facts cannot be re-derived
/// from it (reclassifying trimmed content would always read them as false).
/// The other fields are refreshed only alongside a type change.
///
/// The DB mutex is held only for the initial collect and for one final
/// batched-transaction update. `ContentProcessor::process` calls
/// `Path::is_dir()` / `Path::is_file()` for path-like content, so running it
/// under the lock would block every other DB access (clipboard listener,
/// search) for the whole pass — on a history with many file clips that
/// stalls the entire daemon. Previously each row's `UPDATE` also committed
/// individually (one lock round-trip per clip instead of one for the pass).
pub async fn reclassify_all(db: &Arc<DbPool>) -> Result<u64> {
    let rows = db.with(reclassify_collect).await?;

    // Classification (incl. filesystem existence checks) — no DB lock held.
    let mut updates: Vec<(i64, String, String, i64, i64)> = Vec::new();
    for (id, raw, cur_clip_type, is_copied_file) in rows {
        if raw.trim().is_empty() {
            continue;
        }
        let normalised = normalize_line_endings(&raw);
        if let Some(c) = ContentProcessor::process(&normalised, is_copied_file) {
            let new_type = c.clip_type.as_str();
            if new_type == cur_clip_type {
                continue;
            }
            // A stored file clip whose path is now missing reclassifies to
            // `file_generic` (the classifier can only see current disk state).
            // The specific type reflects what was copied, not what still exists
            // — deadhead maintenance already flags the missing path — so don't
            // degrade it.
            if ClipType::parse(&cur_clip_type).is_file_clip() && new_type == "file_generic" {
                continue;
            }
            updates.push((
                id,
                new_type.to_string(),
                c.preview_content,
                c.size_in_bytes,
                c.is_multiline as i64,
            ));
        }
    }

    if updates.is_empty() {
        return Ok(0);
    }

    let updated = updates.len() as u64;
    db.with(move |conn| {
        let tx = conn.unchecked_transaction()?;
        for (id, clip_type, preview_content, size_in_bytes, is_multiline) in &updates {
            tx.execute(
                "UPDATE clips
                 SET ClipType = ?1, PreviewContent = ?2, SizeInBytes = ?3, IsMultiline = ?4
                 WHERE Id = ?5",
                params![clip_type, preview_content, size_in_bytes, is_multiline, id],
            )?;
        }
        tx.commit()?;
        Ok(())
    })
    .await?;

    info!("reclassify_all: updated {updated} clips");
    Ok(updated)
}
