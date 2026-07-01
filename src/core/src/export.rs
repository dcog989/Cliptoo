//! Export and import of the clip history as JSON.
//!
//! ## Export format
//!
//! JSON — a top-level array of clip objects.  Lossless, round-trippable via
//! serde.  This is the only export/import format; XML and CSV were removed
//! because they added maintenance burden with no consumer demand.
//!
//! ## Import
//!
//! Import is **additive**: rows with a `ContentHash` already in the DB are
//! silently skipped.  `UniqueClipsEver` is incremented by the number of new
//! inserts.

use anyhow::{Context, Result};
use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};
use tracing::info;

// ── Shared row type ───────────────────────────────────────────────────────────

/// A fully-populated clip row, including `Content`.
/// Used for both export serialisation and import deserialisation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExportRow {
    pub id: i64,
    pub content: String,
    pub preview_content: String,
    pub content_hash: String,
    pub clip_type: String,
    pub source_app: Option<String>,
    pub timestamp: String,
    pub is_bookmarked: bool,
    pub was_trimmed: bool,
    pub has_leading_whitespace: bool,
    pub is_multiline: bool,
    pub size_in_bytes: i64,
    pub paste_count: i64,
    pub tags: Option<String>,
}

// ── DB read helper ────────────────────────────────────────────────────────────

/// Fetch all clips from the DB as `ExportRow`s, ordered by Timestamp DESC.
pub fn fetch_all(conn: &Connection) -> Result<Vec<ExportRow>> {
    let mut stmt = conn.prepare_cached(
        "SELECT Id, Content, PreviewContent, ContentHash, ClipType,
                SourceApp, Timestamp, IsBookmarked, WasTrimmed,
                HasLeadingWhitespace, IsMultiline, SizeInBytes, PasteCount, Tags
         FROM clips
         ORDER BY Timestamp DESC",
    )?;

    let rows = stmt
        .query_map([], |row| {
            Ok(ExportRow {
                id: row.get(0)?,
                content: row.get::<_, Option<String>>(1)?.unwrap_or_default(),
                preview_content: row.get::<_, Option<String>>(2)?.unwrap_or_default(),
                content_hash: row.get(3)?,
                clip_type: row.get(4)?,
                source_app: row.get(5)?,
                timestamp: row.get(6)?,
                is_bookmarked: row.get::<_, i32>(7)? != 0,
                was_trimmed: row.get::<_, i32>(8)? != 0,
                has_leading_whitespace: row.get::<_, i32>(9)? != 0,
                is_multiline: row.get::<_, i32>(10)? != 0,
                size_in_bytes: row.get(11)?,
                paste_count: row.get(12)?,
                tags: row.get(13)?,
            })
        })?
        .collect::<std::result::Result<Vec<_>, _>>()
        .context("fetch clips for export")?;

    Ok(rows)
}

// ── JSON ──────────────────────────────────────────────────────────────────────

/// Serialise all clips to a pretty-printed JSON byte vector.
pub fn export_json(conn: &Connection) -> Result<Vec<u8>> {
    let rows = fetch_all(conn)?;
    let json = serde_json::to_vec_pretty(&rows).context("serialise clips to JSON")?;
    info!("export_json: {} clips", rows.len());
    Ok(json)
}

/// Import clips from a JSON byte slice.  Existing rows (by ContentHash) are
/// skipped.  Returns the number of rows actually inserted.
pub fn import_json(conn: &Connection, data: &[u8]) -> Result<u64> {
    let rows: Vec<ExportRow> = serde_json::from_slice(data).context("parse import JSON")?;

    let mut inserted: u64 = 0;
    for row in &rows {
        let exists: bool = match conn.query_row(
            "SELECT 1 FROM clips WHERE ContentHash = ?1",
            params![row.content_hash],
            |_| Ok(true),
        ) {
            Ok(v) => v,
            Err(rusqlite::Error::QueryReturnedNoRows) => false,
            Err(e) => {
                tracing::warn!("import: hash lookup failed for {}: {e}", row.content_hash);
                false
            }
        };

        if exists {
            continue;
        }

        conn.execute(
            "INSERT INTO clips (
                Content, PreviewContent, ContentHash, ClipType, SourceApp,
                Timestamp, IsBookmarked, WasTrimmed, HasLeadingWhitespace,
                IsMultiline, SizeInBytes, PasteCount, Tags
             ) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13)",
            params![
                row.content,
                row.preview_content,
                row.content_hash,
                row.clip_type,
                row.source_app,
                row.timestamp,
                row.is_bookmarked as i32,
                row.was_trimmed as i32,
                row.has_leading_whitespace as i32,
                row.is_multiline as i32,
                row.size_in_bytes,
                row.paste_count,
                row.tags,
            ],
        )
        .with_context(|| format!("insert imported clip hash={}", row.content_hash))?;

        inserted += 1;
    }

    if inserted > 0 {
        conn.execute(
            "UPDATE stats SET Value = CAST(CAST(Value AS INTEGER) + ?1 AS TEXT)
             WHERE Key = 'UniqueClipsEver'",
            params![inserted as i64],
        )
        .context("increment UniqueClipsEver")?;
    }

    info!("import_json: {} / {} rows inserted", inserted, rows.len());
    Ok(inserted)
}

// ── Dispatch helpers ──────────────────────────────────────────────────────────

/// Export the DB to the given path in JSON format.
pub async fn export_to_file(
    db: &std::sync::Arc<crate::db::DbPool>,
    path: &std::path::Path,
) -> Result<usize> {
    let bytes = db.with(export_json).await?;
    let len = bytes.len();
    tokio::fs::write(path, &bytes)
        .await
        .with_context(|| format!("write export to {:?}", path))?;
    info!("export_to_file: wrote {} bytes to {:?}", len, path);
    Ok(len)
}

/// Import a JSON file into the DB.  Returns the number of new rows inserted.
pub async fn import_from_file(
    db: &std::sync::Arc<crate::db::DbPool>,
    path: &std::path::Path,
) -> Result<u64> {
    let bytes = tokio::fs::read(path)
        .await
        .with_context(|| format!("read import file {:?}", path))?;
    db.with(|conn| import_json(conn, &bytes)).await
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn make_row(id: i64, content: &str) -> ExportRow {
        ExportRow {
            id,
            content: content.to_string(),
            preview_content: content[..content.len().min(40)].to_string(),
            content_hash: format!("hash{id:04}"),
            clip_type: "text".to_string(),
            source_app: None,
            timestamp: "2024-01-01T00:00:00".to_string(),
            is_bookmarked: false,
            was_trimmed: false,
            has_leading_whitespace: false,
            is_multiline: content.contains('\n'),
            size_in_bytes: content.len() as i64,
            paste_count: 0,
            tags: None,
        }
    }

    #[test]
    fn json_round_trip() {
        let rows = vec![make_row(1, "hello world"), make_row(2, "foo\nbar")];
        let json = serde_json::to_vec_pretty(&rows).unwrap();
        let back: Vec<ExportRow> = serde_json::from_slice(&json).unwrap();
        assert_eq!(back.len(), 2);
        assert_eq!(back[0].content, "hello world");
        assert_eq!(back[1].content, "foo\nbar");
    }
}
