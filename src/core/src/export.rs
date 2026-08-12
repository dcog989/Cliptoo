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
//! silently skipped.  Rows whose hash is not the canonical 64-char lowercase
//! SHA-256 hex shape are rejected (see `import_json`).  `UniqueClipsEver` is
//! incremented by the number of new inserts.

use anyhow::{Context, Result};
use chrono::{DateTime, NaiveDateTime, Utc};
use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};
use tracing::{info, warn};

use crate::db::queries::{EPOCH_TS_PREFIX, TIMESTAMP_FORMAT, next_timestamp};

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

/// Fetch clip rows as `ExportRow`s, ordered by Timestamp DESC.
/// `where_clause` is appended verbatim (e.g. `"WHERE IsBookmarked = 1"`).
fn fetch_rows(conn: &Connection, where_clause: &str) -> Result<Vec<ExportRow>> {
    let mut stmt = conn.prepare_cached(&format!(
        "SELECT Id, Content, PreviewContent, ContentHash, ClipType,
                SourceApp, Timestamp, IsBookmarked, WasTrimmed,
                HasLeadingWhitespace, IsMultiline, SizeInBytes, PasteCount, Tags
         FROM clips
         {where_clause}
         ORDER BY Timestamp DESC"
    ))?;

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

/// Fetch all clips from the DB as `ExportRow`s.
pub fn fetch_all(conn: &Connection) -> Result<Vec<ExportRow>> {
    fetch_rows(conn, "")
}

/// Fetch only bookmarked clips from the DB as `ExportRow`s.
pub fn fetch_bookmarked(conn: &Connection) -> Result<Vec<ExportRow>> {
    fetch_rows(conn, "WHERE IsBookmarked = 1")
}

// ── JSON ──────────────────────────────────────────────────────────────────────

/// Serialise all clips to a pretty-printed JSON byte vector.
pub fn export_json(conn: &Connection) -> Result<Vec<u8>> {
    let rows = fetch_all(conn)?;
    let json = serde_json::to_vec_pretty(&rows).context("serialise clips to JSON")?;
    info!("export_json: {} clips", rows.len());
    Ok(json)
}

/// Serialise only bookmarked clips to a pretty-printed JSON byte vector.
pub fn export_bookmarked_json(conn: &Connection) -> Result<Vec<u8>> {
    let rows = fetch_bookmarked(conn)?;
    let json = serde_json::to_vec_pretty(&rows).context("serialise bookmarks to JSON")?;
    info!("export_bookmarked_json: {} clips", rows.len());
    Ok(json)
}

/// Normalise an imported clip `Timestamp` to the canonical app format — UTC,
/// space-separated `YYYY-MM-DD HH:MM:SS[.ffffff]`. The ordering, reorder and
/// retention queries compare `Timestamp` strings lexicographically and assume
/// that single shape, so a foreign value (an ISO-8601 `T` separator, or a
/// timezone offset) would otherwise sort out of band and evade the age-based
/// retention sweep.
///
/// Values already in the app's own shape are kept byte-for-byte so an
/// export/import round-trip preserves ordering and pin state exactly — this
/// includes the bottom-pin sentinel, whose 15-digit fraction chrono cannot
/// parse. Foreign values are converted to UTC; unparseable values return
/// `None` and the caller falls back to the current time.
fn normalize_timestamp(s: &str) -> Option<String> {
    // App-written canonical shapes (space-separated; `%.f` accepts both
    // integer and fractional seconds): keep verbatim.
    if NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S%.f").is_ok() {
        return Some(s.to_string());
    }
    // Bottom-pin sentinel: `1970-01-01 00:00:00.{padded Id}`.
    if let Some(rest) = s.strip_prefix(EPOCH_TS_PREFIX)
        && let Some(digits) = rest.strip_prefix('.')
        && !digits.is_empty()
        && digits.bytes().all(|b| b.is_ascii_digit())
    {
        return Some(s.to_string());
    }

    // Foreign formats: RFC3339 / ISO-8601 with a timezone offset, or a `T`-
    // separated naive value; both denote UTC instants. Convert to UTC.
    if let Ok(dt) = DateTime::parse_from_rfc3339(s) {
        return Some(dt.with_timezone(&Utc).format(TIMESTAMP_FORMAT).to_string());
    }
    if let Ok(dt) = DateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S%.f%z") {
        return Some(dt.with_timezone(&Utc).format(TIMESTAMP_FORMAT).to_string());
    }
    if let Ok(ndt) = NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S%.f") {
        return Some(ndt.format(TIMESTAMP_FORMAT).to_string());
    }
    None
}

/// True when `h` matches the canonical ContentHash shape produced by
/// `content::hash::sha256_hex`: a 64-char **lowercase** SHA-256 hex digest.
///
/// `import_json` rejects anything else.  A foreign or corrupt hash would
/// poison the thumbnail/favicon filename keys derived from its first
/// `HASH_FILENAME_PREFIX_LEN` bytes, and — because that derivation is a byte
/// slice over UTF-8 — a hash whose first 16 bytes split a multi-byte char
/// would panic the scheduled `prune_cache` task.  Uppercase hex is rejected
/// too: thumbnail filenames are built from lowercase prefixes, so an
/// uppercase hash would dedup against a different prefix than the files it
/// keys.
pub fn is_valid_content_hash(h: &str) -> bool {
    h.len() == 64
        && h.bytes()
            .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
}

/// Import clips from a JSON byte slice.  Existing rows (by ContentHash) are
/// skipped.  Rows whose `content_hash` is not a canonical 64-char lowercase
/// SHA-256 hex digest are rejected (skipped with a warning).  Returns the
/// number of rows actually inserted.
///
/// Runs as a single explicit transaction with existing hashes preloaded into
/// a `HashSet`, rather than a per-row `SELECT` + autocommitted `INSERT`: for
/// a large import that was N*2 unbatched round-trips (each one committing
/// individually in WAL mode) instead of one lookup pass plus one transaction.
pub fn import_json(conn: &Connection, data: &[u8]) -> Result<u64> {
    let rows: Vec<ExportRow> = serde_json::from_slice(data).context("parse import JSON")?;

    let mut existing_hashes: std::collections::HashSet<String> = {
        let mut stmt = conn.prepare_cached("SELECT ContentHash FROM clips")?;
        stmt.query_map([], |row| row.get::<_, String>(0))?
            .collect::<std::result::Result<_, _>>()
            .context("fetch existing hashes for import")?
    };

    // Single transaction; an uncommitted drop rolls back automatically, so no
    // manual ROLLBACK arms are needed on the error paths below.
    let tx = conn
        .unchecked_transaction()
        .context("begin import transaction")?;

    let mut inserted: u64 = 0;
    for row in &rows {
        // Reject hashes that are not canonical 64-char lowercase SHA-256 hex.
        // A malformed hash would poison the thumbnail/favicon filename keys
        // derived from its first `HASH_FILENAME_PREFIX_LEN` bytes and could
        // panic `prune_cache` if that byte-slice splits a multi-byte UTF-8
        // char; skipping the row keeps the rest of the import usable.
        if !is_valid_content_hash(&row.content_hash) {
            warn!(
                "import: skipping clip with invalid content_hash {:?} (expected 64 lowercase hex chars)",
                row.content_hash
            );
            continue;
        }

        // Also catches duplicate hashes *within* the same import file, since
        // the set is updated below as each row is inserted.
        if existing_hashes.contains(&row.content_hash) {
            continue;
        }

        // Normalise the imported timestamp to the canonical shape before
        // storing (see `normalize_timestamp`); unparseable values fall back
        // to the current time rather than failing the whole import.
        let timestamp = match normalize_timestamp(&row.timestamp) {
            Some(ts) => ts,
            None => {
                warn!(
                    "import: unparseable timestamp {:?} for clip hash {:?}; using current time",
                    row.timestamp, row.content_hash
                );
                next_timestamp()
            }
        };

        tx.execute(
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
                timestamp,
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

        existing_hashes.insert(row.content_hash.clone());
        inserted += 1;
    }

    if inserted > 0 {
        tx.execute(
            "UPDATE stats SET Value = CAST(CAST(Value AS INTEGER) + ?1 AS TEXT)
             WHERE Key = 'UniqueClipsEver'",
            params![inserted as i64],
        )
        .context("increment UniqueClipsEver")?;
    }

    tx.commit().context("commit import transaction")?;

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

/// Export only bookmarked clips to the given path in JSON format.
pub async fn export_bookmarked_to_file(
    db: &std::sync::Arc<crate::db::DbPool>,
    path: &std::path::Path,
) -> Result<usize> {
    let bytes = db.with(export_bookmarked_json).await?;
    let len = bytes.len();
    tokio::fs::write(path, &bytes)
        .await
        .with_context(|| format!("write bookmarks export to {:?}", path))?;
    info!(
        "export_bookmarked_to_file: wrote {} bytes to {:?}",
        len, path
    );
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

    #[test]
    fn normalizes_foreign_timestamps_to_canonical_utc() {
        assert_eq!(
            normalize_timestamp("2024-01-01T00:00:00").unwrap(),
            "2024-01-01 00:00:00.000000"
        );
        assert_eq!(
            normalize_timestamp("2024-01-01T12:34:56.789012").unwrap(),
            "2024-01-01 12:34:56.789012"
        );
        assert_eq!(
            normalize_timestamp("2024-01-01T00:00:00Z").unwrap(),
            "2024-01-01 00:00:00.000000"
        );
        // Timezone offsets are shifted to UTC.
        assert_eq!(
            normalize_timestamp("2024-01-01T02:00:00+02:00").unwrap(),
            "2024-01-01 00:00:00.000000"
        );
        assert_eq!(
            normalize_timestamp("2024-01-01 00:00:00+02:00").unwrap(),
            "2023-12-31 22:00:00.000000"
        );
    }

    #[test]
    fn keeps_canonical_and_bottom_pin_timestamps_verbatim() {
        // App-written shapes round-trip byte-for-byte.
        assert_eq!(
            normalize_timestamp("2024-01-01 00:00:00").unwrap(),
            "2024-01-01 00:00:00"
        );
        assert_eq!(
            normalize_timestamp("2024-01-01 00:00:00.123456").unwrap(),
            "2024-01-01 00:00:00.123456"
        );
        // Bottom-pin sentinel: 15-digit fraction is not a parseable chrono
        // value, but must survive a round-trip unchanged.
        assert_eq!(
            normalize_timestamp("1970-01-01 00:00:00.000000000000015").unwrap(),
            "1970-01-01 00:00:00.000000000000015"
        );
        assert_eq!(
            normalize_timestamp("1970-01-01 00:00:00").unwrap(),
            "1970-01-01 00:00:00"
        );
    }

    #[test]
    fn rejects_unparseable_timestamps() {
        assert_eq!(normalize_timestamp(""), None);
        assert_eq!(normalize_timestamp("not a timestamp"), None);
        assert_eq!(normalize_timestamp("2024-13-45 99:00:00"), None);
    }

    #[test]
    fn accepts_canonical_sha256_hex_hashes() {
        let valid = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
        assert!(is_valid_content_hash(valid));
    }

    #[test]
    fn rejects_malformed_content_hashes() {
        let valid = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
        // Wrong length.
        assert!(!is_valid_content_hash(""));
        assert!(!is_valid_content_hash(&valid[..63]));
        assert!(!is_valid_content_hash(&format!("{valid}x")));
        // Non-hex characters.
        assert!(!is_valid_content_hash(&format!("g{}", &valid[1..])));
        assert!(!is_valid_content_hash(&format!("-{}", &valid[1..])));
        // Uppercase hex is not the canonical lowercase shape.
        assert!(!is_valid_content_hash(&valid.to_uppercase()));
        // Multi-byte UTF-8 — the first 16 bytes split an emoji, so a naive
        // prefix byte-slice would panic.
        assert!(!is_valid_content_hash("123456789012345😀"));
    }
}
