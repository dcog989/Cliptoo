use crate::db::models::ClipData;
use anyhow::Result;
use rusqlite::{Connection, OptionalExtension, params};

// ── FTS5 snippet configuration ──────────────────────────────────────────────
// Tag snippets are shorter because tag values are typically brief labels.
pub const TAG_SNIPPET_TOKENS: i32 = 16;
pub const CONTENT_SNIPPET_TOKENS: i32 = 32;

/// Opening highlight sentinel emitted by FTS5 `snippet()`.
/// The UI crate must use the same delimiters when parsing match spans.
pub const FTS_HL_OPEN: &str = "[HL]";
/// Closing highlight sentinel emitted by FTS5 `snippet()`.
pub const FTS_HL_CLOSE: &str = "[/HL]";

/// Sentinel timestamp used by `bump_to_bottom` — Unix epoch.
/// Clips bumped to bottom sort before anything with a real timestamp.
const EPOCH_TIMESTAMP: &str = "1970-01-01 00:00:00";

/// Default row limit for `search_clips` — bounds the list view.
pub const SEARCH_RESULT_LIMIT: i64 = 1000;

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

fn clip_type_filter(filter: &str) -> FilterClause {
    match filter {
        "bookmarked" => FilterClause::NoParam("AND c.IsBookmarked = 1"),
        "text" | "link" | "file_image" | "color" | "code_snippet" | "folder" | "file_audio"
        | "file_video" | "file_archive" | "file_document" | "file_database" | "file_font"
        | "rtf" => FilterClause::BindFilter("AND c.ClipType = ?"),
        "file" => FilterClause::NoParam(
            "AND c.ClipType IN ('file_generic', 'file_dev', 'file_danger', 'file_text', 'file_link', 'file_system')",
        ),
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
) -> Result<bool> {
    conn.execute(
        "INSERT INTO clips
             (Content, PreviewContent, ContentHash, ClipType, SourceApp, Timestamp,
              WasTrimmed, HasLeadingWhitespace, IsMultiline, SizeInBytes)
         VALUES (?1, ?2, ?3, ?4, ?5, datetime('now'), ?6, ?7, ?8, ?9)
         ON CONFLICT(ContentHash) DO UPDATE SET Timestamp = datetime('now')",
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
        ],
    )?;
    // SQLite guarantees `last_insert_rowid()` is updated to the new rowid on a
    // true INSERT and left unchanged (existing rowid) on DO UPDATE.  Compare it
    // to the row's actual Id to distinguish the two cases.
    let last = conn.last_insert_rowid();
    let existing_id: i64 = conn.query_row(
        "SELECT Id FROM clips WHERE ContentHash = ?1",
        params![content_hash],
        |row| row.get(0),
    )?;
    Ok(existing_id == last)
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
            "SELECT Id, PreviewContent, ContentHash, ClipType, SourceApp, Timestamp,
                    IsBookmarked, WasTrimmed, HasLeadingWhitespace, SizeInBytes, PasteCount, Tags, NULL,
                    IsMultiline, IsDeadhead
             FROM clips c
             WHERE 1=1 {}
             ORDER BY Timestamp DESC
             LIMIT ? OFFSET ?",
            ft.sql()
        );
        return run_search(conn, &sql, &ft, filter, &[], limit, offset);
    }

    // ── Tag-prefix search ─────────────────────────────────────────────────────
    if is_tag_search {
        if effective_query.is_empty() {
            // "##" alone — return all clips that have any tags (no FTS needed).
            let sql = format!(
                "SELECT Id, PreviewContent, ContentHash, ClipType, SourceApp, Timestamp,
                        IsBookmarked, WasTrimmed, HasLeadingWhitespace, SizeInBytes, PasteCount, Tags, NULL,
                        IsMultiline, IsDeadhead
                 FROM clips c
                 WHERE Tags IS NOT NULL AND Tags != '' {}
                 ORDER BY Timestamp DESC
                 LIMIT ? OFFSET ?",
                ft.sql()
            );
            return run_search(conn, &sql, &ft, filter, &[], limit, offset);
        }

        // FTS5 column filter syntax: `Tags : <query>` restricts to the Tags column.
        let fts_query = build_fts_query(effective_query);
        let fts_col_query = format!("Tags : {fts_query}");
        let sql = format!(
            "SELECT c.Id, c.PreviewContent, c.ContentHash, c.ClipType, c.SourceApp, c.Timestamp,
                    c.IsBookmarked, c.WasTrimmed, c.HasLeadingWhitespace, c.SizeInBytes, c.PasteCount, c.Tags,
                    snippet(clips_fts, 1, '{hl_open}', '{hl_close}', '…', {tokens}),
                    c.IsMultiline, c.IsDeadhead
             FROM clips_fts
             JOIN clips c ON c.Id = clips_fts.rowid
             WHERE clips_fts MATCH ? {filter_sql}
             ORDER BY rank
             LIMIT ? OFFSET ?",
            hl_open = FTS_HL_OPEN,
            hl_close = FTS_HL_CLOSE,
            tokens = TAG_SNIPPET_TOKENS,
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
        "SELECT c.Id, c.PreviewContent, c.ContentHash, c.ClipType, c.SourceApp, c.Timestamp,
                c.IsBookmarked, c.WasTrimmed, c.HasLeadingWhitespace, c.SizeInBytes, c.PasteCount, c.Tags,
                snippet(clips_fts, 0, '{hl_open}', '{hl_close}', '…', {tokens}),
                c.IsMultiline, c.IsDeadhead
         FROM clips_fts
         JOIN clips c ON c.Id = clips_fts.rowid
         WHERE clips_fts MATCH ? {filter_sql}
         ORDER BY rank
         LIMIT ? OFFSET ?",
        hl_open = FTS_HL_OPEN,
        hl_close = FTS_HL_CLOSE,
        tokens = CONTENT_SNIPPET_TOKENS,
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
        "UPDATE clips SET Timestamp = datetime('now') WHERE Id = ?1",
        params![id],
    )?;
    Ok(())
}

pub fn increment_paste_count(conn: &Connection, id: i64) -> Result<()> {
    conn.execute(
        "UPDATE clips SET PasteCount = PasteCount + 1 WHERE Id = ?1",
        params![id],
    )?;
    Ok(())
}

/// Atomically timestamps the clip to the top and increments its paste count.
/// Use instead of calling `bump_to_top` + `increment_paste_count` separately.
pub fn record_paste(conn: &Connection, id: i64) -> Result<()> {
    conn.execute(
        "UPDATE clips SET Timestamp = datetime('now'), PasteCount = PasteCount + 1 WHERE Id = ?1",
        params![id],
    )?;
    Ok(())
}

pub fn bump_to_bottom(conn: &Connection, id: i64) -> Result<()> {
    conn.execute(
        &format!(
            "UPDATE clips SET Timestamp = '{ts}' WHERE Id = ?1",
            ts = EPOCH_TIMESTAMP
        ),
        params![id],
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

    if let Some((above_id, above_ts)) = above {
        conn.execute(
            "UPDATE clips SET Timestamp = ?1 WHERE Id = ?2",
            params![&above_ts, id],
        )?;
        conn.execute(
            "UPDATE clips SET Timestamp = ?1 WHERE Id = ?2",
            params![&current_ts, above_id],
        )?;
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

    if let Some((below_id, below_ts)) = below {
        conn.execute(
            "UPDATE clips SET Timestamp = ?1 WHERE Id = ?2",
            params![&below_ts, id],
        )?;
        conn.execute(
            "UPDATE clips SET Timestamp = ?1 WHERE Id = ?2",
            params![&current_ts, below_id],
        )?;
    }
    Ok(())
}
