use crate::db::models::{ClipData, ClipType};
use anyhow::Result;
use chrono::{DateTime, Utc};
use rusqlite::{Connection, OptionalExtension, params};
use std::sync::Mutex;

// ── FTS5 snippet configuration ──────────────────────────────────────────────
// Tag snippets are shorter because tag values are typically brief labels.
pub const TAG_SNIPPET_TOKENS: i32 = 16;
pub const CONTENT_SNIPPET_TOKENS: i32 = 32;

/// Opening highlight sentinel emitted by FTS5 `snippet()`.
/// The UI crate must use the same delimiters when parsing match spans.
pub const FTS_HL_OPEN: &str = "[HL]";
/// Closing highlight sentinel emitted by FTS5 `snippet()`.
pub const FTS_HL_CLOSE: &str = "[/HL]";

/// Timestamp prefix used by `bump_to_bottom` — a fixed epoch date that sorts
/// before every real timestamp. The clip's `Id` is appended as a zero-padded
/// suffix so bottom-pinned clips stay distinct (and therefore reorderable)
/// instead of all sharing one identical timestamp.
///
/// A pinned clip's `Timestamp` is a sentinel, not a real time, so any age or
/// count retention cutoff would otherwise treat it as ancient and sweep it.
/// Scheduled retention therefore exempts these clips (see
/// `maintenance::bottom_pinned_predicate`), treating a manual "move to bottom"
/// like a bookmark — a deliberate keep signal.
pub(crate) const EPOCH_TS_PREFIX: &str = "1970-01-01 00:00:00";
/// Zero-padded width of the `Id` suffix appended to `EPOCH_TS_PREFIX`.
/// Fixed width keeps the padded values sortable numerically.
const EPOCH_TS_SUFFIX_WIDTH: usize = 15;

/// Last reserved write timestamp, microseconds since the Unix epoch.
/// Writes are serialised on the single connection, but the guard still
/// guarantees a strictly-increasing, collision-free value for back-to-back
/// writes landing within the same microsecond — the reorder swaps in
/// `move_up_one` / `move_down_one` depend on stored `Timestamp`s never being
/// equal (equal timestamps would make a swap a no-op).
static LAST_TS_MICROS: Mutex<u64> = Mutex::new(0);

/// Current UTC time as `YYYY-MM-DD HH:MM:SS.ffffff`, strictly increasing
/// across calls. Each call reserves a microsecond slot greater than every
/// previously reserved one, so two writes in the same microsecond still sort
/// in write order.
fn next_timestamp() -> String {
    let now = Utc::now().timestamp_micros() as u64;
    let micros = {
        let mut last = LAST_TS_MICROS
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        *last = (*last).saturating_add(1).max(now);
        *last
    };
    let dt = DateTime::from_timestamp(
        (micros / 1_000_000) as i64,
        ((micros % 1_000_000) as u32) * 1_000,
    )
    .expect("reserved timestamp fits in UTC range");
    dt.format("%Y-%m-%d %H:%M:%S%.6f").to_string()
}

/// Default row limit for `search_clips` — bounds the list view.
pub const SEARCH_RESULT_LIMIT: i64 = 1000;

/// Rank bonus subtracted from FTS5 `rank` for bookmarked clips (lower rank =
/// better match), so bookmarks sort above comparable non-bookmark matches.
/// FTS5 BM25 ranks are small negative floats; a bonus of 1.0 lifts a bookmark
/// above any non-bookmark whose match is at most ~1.0 rank better.
const BOOKMARK_RANK_BONUS: &str = "1.0";

enum FilterClause {
    /// SQL fragment with no extra bind parameter.
    NoParam(&'static str),
    /// SQL fragment whose single `?` is bound to the original filter string.
    BindFilter(&'static str),
}

impl FilterClause {
    fn sql(&self) -> &'static str {
        match self {
            FilterClause::NoParam(s) | FilterClause::BindFilter(s) => s,
        }
    }
}

/// ClipTypes the UI can filter by directly, mapping 1:1 to their stored
/// `as_str()` value. Kept here so the filter strings always match
/// `ClipType::as_str()`; the special-cased `file_document` / `file_text`
/// (shared filter) and `file_generic` (bare "file" filter) are handled
/// explicitly in `clip_type_filter`.
const FILTERABLE_TYPES: &[ClipType] = &[
    ClipType::Text,
    ClipType::FilePath,
    ClipType::Rtf,
    ClipType::Link,
    ClipType::Color,
    ClipType::CodeSnippet,
    ClipType::FileImage,
    ClipType::FileVideo,
    ClipType::FileAudio,
    ClipType::FileArchive,
    ClipType::FileDev,
    ClipType::FileDanger,
    ClipType::FileData,
    ClipType::Folder,
];

fn clip_type_filter(filter: &str) -> FilterClause {
    match filter {
        "bookmarked" => FilterClause::NoParam("AND c.IsBookmarked = 1"),
        // Document covers .docx/.pdf (file_document) and plain text files
        // (.txt/.md, file_text) — the same icon, so they share a filter.
        "file_document" => {
            FilterClause::NoParam("AND c.ClipType IN ('file_document', 'file_text')")
        }
        // A copied generic file: one that isn't any specific type.
        "file" => FilterClause::NoParam("AND c.ClipType = 'file_generic'"),
        _ if FILTERABLE_TYPES.iter().any(|t| t.as_str() == filter) => {
            FilterClause::BindFilter("AND c.ClipType = ?")
        }
        _ => FilterClause::NoParam(""),
    }
}

fn row_to_clipdata(row: &rusqlite::Row) -> rusqlite::Result<ClipData> {
    Ok(ClipData {
        id: row.get(0)?,
        preview_content: row.get(1)?,
        content_hash: row.get(2)?,
        clip_type: crate::db::models::ClipType::parse(&row.get::<_, String>(3)?),
        source_app: row.get(4)?,
        timestamp: row.get(5)?,
        is_bookmarked: row.get::<_, i32>(6)? != 0,
        was_trimmed: row.get::<_, i32>(7)? != 0,
        has_leading_whitespace: row.get::<_, i32>(8)? != 0,
        size_in_bytes: row.get(9)?,
        paste_count: row.get(10)?,
        tags: row.get(11)?,
        match_context: row.get(12)?,
        is_multiline: row.get::<_, i32>(13)? != 0,
        is_deadhead: row.get::<_, i32>(14)? != 0,
    })
}

/// Wrap each whitespace-delimited term in double-quotes with a `*` suffix for
/// FTS5 prefix matching. Embedded double-quotes are escaped by doubling them,
/// making it safe to pass arbitrary user input as an FTS5 MATCH expression.
fn build_fts_query(query: &str) -> String {
    query
        .split_whitespace()
        .map(|term| format!("\"{}\"*", term.replace('"', "\"\"")))
        .collect::<Vec<_>>()
        .join(" ")
}

/// Execute a pre-built search SQL string and collect the rows.
///
/// `extra_params` are bound first (e.g. the FTS MATCH expression), then
/// `filter` if `ft` is `BindFilter`, then `limit` and `offset`.
/// Using `prepare_cached` means SQLite re-uses the compiled statement across
/// calls — important on the keystroke hot-path.
fn run_search(
    conn: &Connection,
    sql: &str,
    ft: &FilterClause,
    filter: &str,
    extra_params: &[&dyn rusqlite::ToSql],
    limit: i64,
    offset: i64,
) -> Result<Vec<ClipData>> {
    let mut stmt = conn.prepare_cached(sql)?;

    // Build the full parameter list: [extra...] [filter?] limit offset
    let mut p: Vec<&dyn rusqlite::ToSql> = extra_params.to_vec();
    if let FilterClause::BindFilter(_) = ft {
        p.push(&filter);
    }
    p.push(&limit);
    p.push(&offset);

    stmt.query_map(p.as_slice(), row_to_clipdata)?
        .map(|r| r.map_err(anyhow::Error::from))
        .collect()
}

/// Insert a new clip or, if `ContentHash` already exists, bump its `Timestamp`
/// to now (bringing it to the top of the list).
///
/// Returns `true` if a new row was inserted, `false` if an existing row was
/// bumped.  Both statements run inside the caller's `db.with(|conn| ...)` lock,
/// so they are serialised against all other writes on the single connection.
#[allow(clippy::too_many_arguments)]
pub fn insert_or_bump(
    conn: &Connection,
    content: &str,
    preview_content: &str,
    content_hash: &str,
    clip_type: &str,
    source_app: Option<&str>,
    was_trimmed: bool,
    has_leading_whitespace: bool,
    is_multiline: bool,
    size_in_bytes: i64,
    is_file_uri: bool,
) -> Result<bool> {
    // Determine insert-vs-bump by checking existence *before* the upsert,
    // rather than comparing `last_insert_rowid()` to the row's `Id` afterwards.
    // The latter is unreliable on a connection's very first statement: SQLite
    // only guarantees `last_insert_rowid()` is "left unchanged" on the
    // DO-UPDATE branch, and on a fresh connection "unchanged" means it is
    // still 0 — which would wrongly compare unequal to a real existing Id and
    // report a duplicate as newly inserted.
    let already_existed: bool = conn
        .query_row(
            "SELECT 1 FROM clips WHERE ContentHash = ?1",
            params![content_hash],
            |_| Ok(()),
        )
        .optional()?
        .is_some();

    let now = next_timestamp();
    conn.execute(
        "INSERT INTO clips
             (Content, PreviewContent, ContentHash, ClipType, SourceApp, Timestamp,
              WasTrimmed, HasLeadingWhitespace, IsMultiline, SizeInBytes, IsFileUri)
         VALUES (?1, ?2, ?3, ?4, ?5, ?11, ?6, ?7, ?8, ?9, ?10)
         ON CONFLICT(ContentHash) DO UPDATE SET Timestamp = ?11",
        params![
            content,
            preview_content,
            content_hash,
            clip_type,
            source_app,
            was_trimmed as i32,
            has_leading_whitespace as i32,
            is_multiline as i32,
            size_in_bytes,
            is_file_uri as i32,
            now,
        ],
    )?;

    Ok(!already_existed)
}

/// Shared SELECT projections for `search_clips`, kept in `row_to_clipdata`
/// column order (indices 0–14).
///
/// Browse-mode projection (plain `clips` scan). Column 13 is `NULL` because
/// there is no FTS match to highlight.
const BROWSE_PROJECTION: &str = "Id, PreviewContent, ContentHash, ClipType, SourceApp, Timestamp,
        IsBookmarked, WasTrimmed, HasLeadingWhitespace, SizeInBytes, PasteCount, Tags, NULL,
        IsMultiline, IsDeadhead";

/// FTS-mode projection — `c.`-prefixed (JOIN against `clips`) with the
/// highlight snippet in the match-context column.
fn fts_projection(fts_col: i32, tokens: i32) -> String {
    format!(
        "c.Id, c.PreviewContent, c.ContentHash, c.ClipType, c.SourceApp, c.Timestamp,
         c.IsBookmarked, c.WasTrimmed, c.HasLeadingWhitespace, c.SizeInBytes, c.PasteCount, c.Tags,
         snippet(clips_fts, {fts_col}, '{hl_open}', '{hl_close}', '…', {tokens}),
         c.IsMultiline, c.IsDeadhead",
        hl_open = FTS_HL_OPEN,
        hl_close = FTS_HL_CLOSE,
    )
}

/// Search clips by full-text query and/or type filter.
///
/// `tag_prefix`: when the raw user query starts with this string, the search is
/// routed to the FTS5 `Tags` column instead of `Content`.  Pass `None` (or `""`)
/// to always search `Content`.
pub fn search_clips(
    conn: &Connection,
    query: &str,
    filter: &str,
    limit: i64,
    offset: i64,
    tag_prefix: Option<&str>,
) -> Result<Vec<ClipData>> {
    let ft = clip_type_filter(filter);

    // Determine whether this is a tag search and, if so, strip the prefix.
    let (is_tag_search, effective_query) =
        match tag_prefix.filter(|p| !p.is_empty() && query.starts_with(*p)) {
            Some(pfx) => (true, query[pfx.len()..].trim()),
            None => (false, query),
        };

    // ── Browse (no query text) ────────────────────────────────────────────────
    if effective_query.is_empty() && !is_tag_search {
        let sql = format!(
            "SELECT {projection}
             FROM clips c
             WHERE 1=1 {filter_sql}
             ORDER BY Timestamp DESC, Id DESC
             LIMIT ? OFFSET ?",
            projection = BROWSE_PROJECTION,
            filter_sql = ft.sql(),
        );
        return run_search(conn, &sql, &ft, filter, &[], limit, offset);
    }

    // ── Tag-prefix search ─────────────────────────────────────────────────────
    if is_tag_search {
        if effective_query.is_empty() {
            // "##" alone — return all clips that have any tags (no FTS needed).
            let sql = format!(
                "SELECT {projection}
                 FROM clips c
                 WHERE Tags IS NOT NULL AND Tags != '' {filter_sql}
                 ORDER BY Timestamp DESC, Id DESC
                 LIMIT ? OFFSET ?",
                projection = BROWSE_PROJECTION,
                filter_sql = ft.sql(),
            );
            return run_search(conn, &sql, &ft, filter, &[], limit, offset);
        }

        // FTS5 column filter syntax: `Tags : <query>` restricts to the Tags column.
        let fts_query = build_fts_query(effective_query);
        let fts_col_query = format!("Tags : {fts_query}");
        let sql = format!(
            "SELECT {projection}
             FROM clips_fts
             JOIN clips c ON c.Id = clips_fts.rowid
             WHERE clips_fts MATCH ? {filter_sql}
             ORDER BY rank - (c.IsBookmarked * {BOOKMARK_RANK_BONUS})
             LIMIT ? OFFSET ?",
            projection = fts_projection(1, TAG_SNIPPET_TOKENS),
            filter_sql = ft.sql(),
        );
        return run_search(conn, &sql, &ft, filter, &[&fts_col_query], limit, offset);
    }

    // ── Full-text content search ──────────────────────────────────────────────
    // Wrap each whitespace-delimited term in double-quotes (escaping embedded
    // double-quotes by doubling them) and append `*` for prefix matching.
    // This gives incremental results as the user types while safely handling
    // FTS5 special characters.
    let fts_query = build_fts_query(query);
    let sql = format!(
        "SELECT {projection}
         FROM clips_fts
         JOIN clips c ON c.Id = clips_fts.rowid
         WHERE clips_fts MATCH ? {filter_sql}
         ORDER BY rank - (c.IsBookmarked * {BOOKMARK_RANK_BONUS})
         LIMIT ? OFFSET ?",
        projection = fts_projection(0, CONTENT_SNIPPET_TOKENS),
        filter_sql = ft.sql(),
    );
    run_search(conn, &sql, &ft, filter, &[&fts_query], limit, offset)
}

pub fn get_clip_content(conn: &Connection, id: i64) -> Result<String> {
    let content: String = conn.query_row(
        "SELECT Content FROM clips WHERE Id = ?1",
        params![id],
        |row| row.get(0),
    )?;
    Ok(content)
}

pub fn get_clip_type_and_content(conn: &Connection, id: i64) -> Result<(String, String, String)> {
    let row: (String, String, String) = conn.query_row(
        "SELECT Content, ClipType, ContentHash FROM clips WHERE Id = ?1",
        params![id],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
    )?;
    Ok(row)
}

pub fn get_clip_tags(conn: &Connection, id: i64) -> Result<String> {
    let tags: Option<String> =
        conn.query_row("SELECT Tags FROM clips WHERE Id = ?1", params![id], |row| {
            row.get(0)
        })?;
    Ok(tags.unwrap_or_default())
}

pub fn delete_clip(conn: &Connection, id: i64) -> Result<()> {
    conn.execute("DELETE FROM clips WHERE Id = ?1", params![id])?;
    Ok(())
}

pub fn set_bookmarked(conn: &Connection, id: i64, value: bool) -> Result<()> {
    conn.execute(
        "UPDATE clips SET IsBookmarked = ?1 WHERE Id = ?2",
        params![value as i32, id],
    )?;
    Ok(())
}

pub fn update_tags(conn: &Connection, id: i64, tags: &str) -> Result<()> {
    let tags = if tags.trim().is_empty() {
        None
    } else {
        Some(tags.to_string())
    };
    conn.execute(
        "UPDATE clips SET Tags = ?1 WHERE Id = ?2",
        params![tags, id],
    )?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub fn update_clip_content(
    conn: &Connection,
    id: i64,
    content: &str,
    preview_content: &str,
    content_hash: &str,
    clip_type: &str,
    was_trimmed: bool,
    has_leading_whitespace: bool,
    is_multiline: bool,
    size_in_bytes: i64,
) -> Result<()> {
    conn.execute(
        "UPDATE clips SET Content = ?1, PreviewContent = ?2, ContentHash = ?3, ClipType = ?4,
         WasTrimmed = ?5, HasLeadingWhitespace = ?6, IsMultiline = ?7, SizeInBytes = ?8
         WHERE Id = ?9",
        params![
            content,
            preview_content,
            content_hash,
            clip_type,
            was_trimmed as i32,
            has_leading_whitespace as i32,
            is_multiline as i32,
            size_in_bytes,
            id,
        ],
    )?;
    Ok(())
}

pub fn bump_to_top(conn: &Connection, id: i64) -> Result<()> {
    conn.execute(
        "UPDATE clips SET Timestamp = ?1 WHERE Id = ?2",
        params![next_timestamp(), id],
    )?;
    Ok(())
}

/// Total number of clips currently stored.
pub fn count_clips(conn: &Connection) -> Result<i64> {
    let n = conn.query_row("SELECT COUNT(*) FROM clips", [], |row| row.get(0))?;
    Ok(n)
}

/// Atomically timestamps the clip to the top and increments its paste count.
/// Also bumps the global paste counter. Use instead of calling `bump_to_top`
/// separately.
pub fn record_paste(conn: &Connection, id: i64) -> Result<()> {
    conn.execute(
        "UPDATE clips SET Timestamp = ?1, PasteCount = PasteCount + 1 WHERE Id = ?2",
        params![next_timestamp(), id],
    )?;
    crate::stats::increment_stat(conn, crate::stats::KEY_PASTE_COUNT)
}

pub fn bump_to_bottom(conn: &Connection, id: i64) -> Result<()> {
    let ts = format!(
        "{EPOCH_TS_PREFIX}.{id:0width$}",
        width = EPOCH_TS_SUFFIX_WIDTH
    );
    conn.execute(
        "UPDATE clips SET Timestamp = ?1 WHERE Id = ?2",
        params![ts, id],
    )?;
    Ok(())
}

pub fn move_up_one(conn: &Connection, id: i64) -> Result<()> {
    let current_ts: String = conn.query_row(
        "SELECT Timestamp FROM clips WHERE Id = ?1",
        params![id],
        |row| row.get(0),
    )?;

    let above = conn
        .query_row(
            "SELECT Id, Timestamp FROM clips
             WHERE (Timestamp, Id) > (?1, ?2)
             ORDER BY Timestamp ASC, Id ASC
             LIMIT 1",
            params![&current_ts, id],
            |row| Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?)),
        )
        .optional()?;

    // Swap the two clips' timestamps inside one transaction: a crash between
    // the pair of writes would otherwise leave both rows holding the same
    // timestamp. The reads above run under the caller's connection lock, so
    // no write can interleave between them and this transaction.
    if let Some((above_id, above_ts)) = above {
        let tx = conn.unchecked_transaction()?;
        tx.execute(
            "UPDATE clips SET Timestamp = ?1 WHERE Id = ?2",
            params![&above_ts, id],
        )?;
        tx.execute(
            "UPDATE clips SET Timestamp = ?1 WHERE Id = ?2",
            params![&current_ts, above_id],
        )?;
        tx.commit()?;
    }
    Ok(())
}

pub fn move_down_one(conn: &Connection, id: i64) -> Result<()> {
    let current_ts: String = conn.query_row(
        "SELECT Timestamp FROM clips WHERE Id = ?1",
        params![id],
        |row| row.get(0),
    )?;

    let below = conn
        .query_row(
            "SELECT Id, Timestamp FROM clips
             WHERE (Timestamp, Id) < (?1, ?2)
             ORDER BY Timestamp DESC, Id DESC
             LIMIT 1",
            params![&current_ts, id],
            |row| Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?)),
        )
        .optional()?;

    // Same atomicity argument as `move_up_one`: both writes land in a single
    // transaction so an interrupted swap cannot leave duplicated timestamps.
    if let Some((below_id, below_ts)) = below {
        let tx = conn.unchecked_transaction()?;
        tx.execute(
            "UPDATE clips SET Timestamp = ?1 WHERE Id = ?2",
            params![&below_ts, id],
        )?;
        tx.execute(
            "UPDATE clips SET Timestamp = ?1 WHERE Id = ?2",
            params![&current_ts, below_id],
        )?;
        tx.commit()?;
    }
    Ok(())
}
