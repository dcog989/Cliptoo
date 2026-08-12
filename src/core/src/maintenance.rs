//! Data lifecycle maintenance tasks.
//!
//! All functions take a `&DbPool` so they share the single WAL connection.
//! Scheduled tasks run on a Tokio background task; manual tasks are triggered
//! by the UI via `on_maintenance_action`.

use anyhow::Result;
use rusqlite::{Connection, params};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tracing::{info, warn};

use crate::content::classifier::ContentProcessor;
use crate::content::hash::normalize_line_endings;
use crate::db::DbPool;
use crate::db::models::ClipType;
use crate::db::queries::EPOCH_TS_PREFIX;
use crate::stats;
use crate::time::utc_now_iso;

use crate::image::HASH_FILENAME_PREFIX_LEN;

// ── Public API ────────────────────────────────────────────────────────────────

/// Parameters for the scheduled retention pass.
#[derive(Clone)]
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

/// Extract the host (domain) from an absolute URL, matching the exact rule the
/// UI uses to name favicon cache files (`{domain}.webp`, see
/// `src/ui/src/favicon.rs`). Returns `None` for relative or scheme-less
/// strings — the UI never caches a favicon for those, so no file keyed by them
/// can exist to prune. Must stay in sync with `helpers::extract_domain`.
fn extract_domain(url: &str) -> Option<String> {
    url::Url::parse(url).ok()?.host_str().map(ToOwned::to_owned)
}

/// Collect the favicon cache keys still referenced by a stored Link clip.
/// Favicons are keyed by domain, not content hash, so membership here is
/// checked against Link-clip domains rather than hash prefixes.
fn known_favicon_domains(conn: &Connection) -> Result<HashSet<String>> {
    let mut stmt = conn.prepare_cached("SELECT Content FROM clips WHERE ClipType = 'link'")?;
    let mut domains = HashSet::new();
    for row in stmt.query_map([], |row| row.get::<_, Option<String>>(0))? {
        match row {
            Ok(Some(content)) => {
                if let Some(domain) = extract_domain(&content) {
                    domains.insert(domain);
                }
            }
            Ok(None) => {}
            Err(e) => warn!("prune_cache: link content read error: {e}"),
        }
    }
    Ok(domains)
}

/// True when a favicon-cache filename belongs to no stored Link clip.
///
/// Cache files are `{domain}.webp`, `{domain}.dark.webp` (dark-theme variant)
/// and `{domain}.title` (cached page title). A file is orphaned when none of
/// its candidate domain keys appears in `known_domains`. Candidates cover the
/// `.dark` suffix ambiguity (a domain that itself ends in `.dark`) so a live
/// file is never pruned because of a suffix that was appended to its name.
/// Files with unrecognised extensions are treated as orphans — the directory
/// is fully managed by the favicon cache.
fn favicon_is_orphan(name: &str, known_domains: &HashSet<String>) -> bool {
    let candidates = if let Some(domain) = name.strip_suffix(".dark.webp") {
        vec![
            domain.to_string(),
            name.strip_suffix(".webp").unwrap_or(name).to_string(),
        ]
    } else if let Some(domain) = name.strip_suffix(".webp") {
        vec![domain.to_string()]
    } else if let Some(domain) = name.strip_suffix(".title") {
        vec![domain.to_string()]
    } else {
        return true;
    };
    candidates.iter().all(|c| !known_domains.contains(c))
}

/// Delete thumbnail and favicon files on disk that have no matching DB record.
/// Returns the total number of files deleted.
///
/// Thumbnails are keyed by the first 16 chars of `ContentHash`; favicon cache
/// files are keyed by the domain of a stored Link clip — the two dirs are
/// matched against their respective keys, so a favicon for a still-present
/// link is never pruned (the previous hash-prefix-only match deleted every
/// favicon on each run).
pub async fn prune_cache(
    db: &Arc<DbPool>,
    thumbnails_dir: &Path,
    favicons_dir: &Path,
) -> Result<u64> {
    // Collect the 16-char hash prefixes and the Link-clip domains that are
    // still in the DB. Storing the compact keys lets both dirs use a direct
    // HashSet::contains lookup — O(1) per file instead of O(n).
    let (known_prefixes, known_domains) = db
        .with(|conn| {
            let mut stmt = conn.prepare_cached("SELECT ContentHash FROM clips")?;
            let prefixes: HashSet<String> = stmt
                .query_map([], |row| row.get::<_, String>(0))?
                .filter_map(|r| r.ok())
                .filter_map(|h| {
                    let end = h.len().min(HASH_FILENAME_PREFIX_LEN);
                    if h.is_char_boundary(end) {
                        Some(h[..end].to_string())
                    } else {
                        // Corrupt/legacy hash (e.g. imported before hash
                        // validation): the prefix slice would split a
                        // multi-byte UTF-8 char. It can never have keyed a
                        // real thumbnail, so it is not a known prefix.
                        warn!(
                            "prune_cache: clip has non-UTF-8-boundary ContentHash; ignoring: {h:?}"
                        );
                        None
                    }
                })
                .collect();
            let domains = known_favicon_domains(conn)?;
            Ok((prefixes, domains))
        })
        .await?;

    let mut pruned: u64 = 0;

    // Thumbnail files are named `{hash[..16]}.webp` (list-cell) or
    // `{hash[..16]}_preview.webp` (preview). Strip the extension, then
    // strip an optional `_preview` suffix to recover the bare 16-char
    // hash prefix before checking DB membership.
    if let Ok(entries) = std::fs::read_dir(thumbnails_dir) {
        for entry in entries.filter_map(|e| e.ok()) {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            let stem = name.rsplit_once('.').map(|(s, _)| s).unwrap_or(&name);
            let hash_prefix = stem.strip_suffix("_preview").unwrap_or(stem);
            if known_prefixes.contains(hash_prefix) {
                continue;
            }
            if let Err(e) = tokio::fs::remove_file(entry.path()).await {
                warn!("prune_cache: failed to remove {:?}: {e}", entry.path());
            } else {
                pruned += 1;
            }
        }
    }

    // Favicons are keyed by Link-clip domain, not by hash.
    if let Ok(entries) = std::fs::read_dir(favicons_dir) {
        for entry in entries.filter_map(|e| e.ok()) {
            let fname = entry.file_name();
            let name = fname.to_string_lossy();
            if !favicon_is_orphan(&name, &known_domains) {
                continue;
            }
            if let Err(e) = tokio::fs::remove_file(entry.path()).await {
                warn!("prune_cache: failed to remove {:?}: {e}", entry.path());
            } else {
                pruned += 1;
            }
        }
    }
    Ok(pruned)
}

/// Delete all non-bookmarked clips. Bookmarked clips are deliberate keep
/// signals, so they always survive a history clear. Returns rows deleted.
pub fn clear_history(conn: &Connection) -> Result<u64> {
    let n = conn.execute("DELETE FROM clips WHERE IsBookmarked = 0", [])? as u64;
    info!("clear_history: removed {n} clips");
    Ok(n)
}

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

/// Spawn the background maintenance task. Runs every `interval` seconds.
/// Retention parameters are read from `retention_rx` at each pass, so a change
/// made in the settings UI applies on the next run without a restart.
pub fn spawn_scheduler(
    db: Arc<DbPool>,
    thumbnails_dir: PathBuf,
    favicons_dir: PathBuf,
    retention_rx: tokio::sync::watch::Receiver<RetentionConfig>,
    interval_secs: u64,
) {
    tokio::spawn(async move {
        // Stagger first run by one interval so startup isn't burdened.
        tokio::time::sleep(std::time::Duration::from_secs(interval_secs)).await;
        loop {
            let cfg = retention_rx.borrow().clone();
            if let Err(e) = run_scheduled(&db, cfg, &thumbnails_dir, &favicons_dir).await {
                warn!("scheduled maintenance error: {e}");
            }
            tokio::time::sleep(std::time::Duration::from_secs(interval_secs)).await;
        }
    });
}

#[cfg(test)]
mod tests {
    use super::{deadhead_clip_types, extract_domain, favicon_is_orphan};
    use std::collections::HashSet;

    #[test]
    fn deadhead_list_covers_expected_file_clip_types() {
        // In `ClipType::ALL` declaration order, filtered to file clips only.
        assert_eq!(
            deadhead_clip_types(),
            "'file_image', 'file_video', 'file_audio', 'file_archive', 'file_document', \
             'file_dev', 'file_danger', 'file_data', 'file_text', 'file_generic', 'folder'"
        );
    }

    #[test]
    fn extracts_host_from_http_urls() {
        assert_eq!(
            extract_domain("https://github.com/foo").unwrap(),
            "github.com"
        );
        assert_eq!(extract_domain("http://example.com").unwrap(), "example.com");
        assert_eq!(
            extract_domain("ftp://files.example.com/x").unwrap(),
            "files.example.com"
        );
    }

    #[test]
    fn normalizes_scheme_and_host_case() {
        assert_eq!(
            extract_domain("HTTP://EXAMPLE.COM/path").unwrap(),
            "example.com"
        );
    }

    #[test]
    fn strips_userinfo_port_and_handles_ipv6() {
        assert_eq!(
            extract_domain("https://user:pass@example.com").unwrap(),
            "example.com"
        );
        assert_eq!(
            extract_domain("https://example.com:8443/x").unwrap(),
            "example.com"
        );
        assert_eq!(extract_domain("https://[::1]:8080/x").unwrap(), "[::1]");
    }

    #[test]
    fn rejects_urls_without_a_host() {
        // The UI never caches a favicon for these, so no file exists to prune.
        for s in [
            "",
            "not a url",
            "www.example.com",
            "example.com",
            "/relative/path",
            "mailto:user@example.com",
        ] {
            assert_eq!(extract_domain(s), None, "for {s:?}");
        }
    }

    fn known(domains: &[&str]) -> HashSet<String> {
        domains.iter().map(|d| d.to_string()).collect()
    }

    #[test]
    fn keeps_favicons_for_live_link_domains() {
        let set = known(&["github.com", "example.com"]);
        assert!(!favicon_is_orphan("github.com.webp", &set));
        assert!(!favicon_is_orphan("github.com.dark.webp", &set));
        assert!(!favicon_is_orphan("example.com.title", &set));
    }

    #[test]
    fn prunes_favicons_whose_domain_is_gone() {
        let set = known(&["github.com"]);
        assert!(favicon_is_orphan("example.com.webp", &set));
        assert!(favicon_is_orphan("example.com.dark.webp", &set));
        assert!(favicon_is_orphan("example.com.title", &set));
        assert!(
            favicon_is_orphan("github.com", &set),
            "bare name is not a cache file"
        );
    }

    #[test]
    fn keeps_domain_that_ends_in_dark() {
        // A domain ending in ".dark" is saved as "dark.dark.webp"; both the
        // ".dark.webp"-stripped and ".webp"-stripped keys must match it.
        let set = known(&["dark.dark"]);
        assert!(!favicon_is_orphan("dark.dark.webp", &set));
        assert!(!favicon_is_orphan("dark.dark.dark.webp", &set));
    }
}
