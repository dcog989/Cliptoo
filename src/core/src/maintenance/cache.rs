//! On-disk cache pruning: thumbnail and favicon files with no matching DB
//! record.

use anyhow::Result;
use rusqlite::{Connection, params};
use std::collections::HashSet;
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, SystemTime};
use tracing::warn;

use crate::content::hash::sha256_hex_and_prefix;
use crate::db::DbPool;
use crate::db::models::ClipType;
use crate::image::HASH_FILENAME_PREFIX_LEN;
use crate::url::extract_domain;

/// Files touched since (pass start - this slack) are left alone. An image
/// capture writes its PNG and thumbnails *before* it inserts the row, so a
/// file created while this pass runs has no DB row yet and must not be
/// deleted; the next pass cleans it if it stays orphaned.
const RECENT_FILE_SLACK: Duration = Duration::from_secs(60);

/// Collect the cache keys still referenced by a stored Link clip: the favicon
/// domain, and the first [`HASH_FILENAME_PREFIX_LEN`] chars of the SHA-256 of
/// the URL, which is the page-title cache key (see `ui/src/favicon.rs`). A
/// title file is not keyed by domain — matching it against domains would prune
/// every title on every pass.
fn known_favicon_keys(conn: &Connection) -> Result<HashSet<String>> {
    let mut stmt = conn.prepare_cached("SELECT Content FROM clips WHERE ClipType = ?1")?;
    let mut keys = HashSet::new();
    for row in stmt.query_map(params![ClipType::Link.as_str()], |row| {
        row.get::<_, Option<String>>(0)
    })? {
        match row {
            Ok(Some(content)) => {
                if let Some(domain) = extract_domain(&content) {
                    keys.insert(domain);
                }
                let (hash, _) = sha256_hex_and_prefix(&content);
                if let Some(stem) = hash.get(..HASH_FILENAME_PREFIX_LEN) {
                    keys.insert(stem.to_string());
                }
            }
            Ok(None) => {}
            Err(e) => warn!("prune_cache: link content read error: {e}"),
        }
    }
    Ok(keys)
}

/// True when a favicon-cache filename belongs to no stored Link clip.
///
/// Cache files are `{domain}.webp`, `{domain}.dark.webp` (dark-theme variant)
/// and `{url-hash}.title` (cached page title). A file is orphaned when none of
/// its candidate keys appears in `known`. Candidates for the dark variant cover
/// the suffix ambiguity (a domain that itself ends in `.dark`) so a live file
/// is never pruned because of a suffix that was appended to its name. Files
/// with unrecognised extensions are treated as orphans — the directory is
/// fully managed by the favicon cache.
fn favicon_is_orphan(name: &str, known: &HashSet<String>) -> bool {
    let candidates = if let Some(domain) = name.strip_suffix(".dark.webp") {
        vec![
            domain.to_string(),
            name.strip_suffix(".webp").unwrap_or(name).to_string(),
        ]
    } else if let Some(domain) = name.strip_suffix(".webp") {
        vec![domain.to_string()]
    } else if let Some(stem) = name.strip_suffix(".title") {
        vec![stem.to_string()]
    } else {
        return true;
    };
    candidates.iter().all(|c| !known.contains(c))
}

/// True when `entry` was written or modified so recently that it may still be
/// mid-ingest. A metadata error is treated as recent, to keep the file.
fn is_recent(entry: &std::fs::DirEntry, cutoff: SystemTime) -> bool {
    entry
        .metadata()
        .and_then(|m| m.modified())
        .map(|t| t >= cutoff)
        .unwrap_or(true)
}

/// Delete thumbnail, favicon and full-size image files on disk that have no
/// matching DB record. Returns the total number of files deleted.
///
/// Thumbnails and full-size images are keyed by the first 16 chars of
/// `ContentHash`; favicon cache files are keyed by the domain of a stored Link
/// clip and title files by the URL hash — each dir is matched against its own
/// key, so a favicon for a still-present link is never pruned.
pub async fn prune_cache(
    db: &Arc<DbPool>,
    thumbnails_dir: &Path,
    favicons_dir: &Path,
    images_dir: &Path,
) -> Result<u64> {
    // Anything written this recently may belong to a capture that has not
    // inserted its row yet (the listener writes the files first).
    let cutoff = SystemTime::now()
        .checked_sub(RECENT_FILE_SLACK)
        .unwrap_or(SystemTime::UNIX_EPOCH);

    // Collect the 16-char hash prefixes, the Link-clip favicon keys and the
    // live clip ids that are still in the DB. Storing the compact keys lets
    // both dirs use a direct HashSet::contains lookup — O(1) per file instead
    // of O(n). The ids prune the favicon failure ledger, which is keyed by clip
    // id and would otherwise keep a row for every clip retention deletes.
    let (known_prefixes, known_favicon_keys, live_ids) = db
        .with(|conn| {
            let mut stmt = conn.prepare_cached("SELECT Id, ContentHash FROM clips")?;
            let mut prefixes: HashSet<String> = HashSet::new();
            let mut live_ids: HashSet<i64> = HashSet::new();
            for row in stmt.query_map([], |row| {
                Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
            })? {
                let Ok((id, hash)) = row else { continue };
                live_ids.insert(id);
                let end = hash.len().min(HASH_FILENAME_PREFIX_LEN);
                if hash.is_char_boundary(end) {
                    prefixes.insert(hash[..end].to_string());
                } else {
                    // Corrupt/legacy hash (e.g. imported before hash
                    // validation): the prefix slice would split a multibyte
                    // UTF-8 char. It can never have keyed a real thumbnail, so
                    // it is not a known prefix.
                    warn!(
                        "prune_cache: clip has non-UTF-8-boundary ContentHash; ignoring: {hash:?}"
                    );
                }
            }
            let favicon_keys = known_favicon_keys(conn)?;
            Ok((prefixes, favicon_keys, live_ids))
        })
        .await?;

    crate::favicon_failures::prune(favicons_dir, &live_ids);

    let mut pruned: u64 = 0;

    // Thumbnail files are named `{hash[..16]}.webp` (list-cell) or
    // `{hash[..16]}_preview.webp` (preview). Strip the extension, then
    // strip an optional `_preview` suffix to recover the bare 16-char
    // hash prefix before checking DB membership.
    if let Ok(entries) = std::fs::read_dir(thumbnails_dir) {
        for entry in entries.filter_map(|e| e.ok()) {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            // Cache bookkeeping files (e.g. `.preview_ratio`) are not keyed by
            // a clip hash; deleting them would force a needless preview rebuild.
            if name.starts_with('.') {
                continue;
            }
            let stem = name.rsplit_once('.').map(|(s, _)| s).unwrap_or(&name);
            let hash_prefix = stem.strip_suffix("_preview").unwrap_or(stem);
            if known_prefixes.contains(hash_prefix) || is_recent(&entry, cutoff) {
                continue;
            }
            if let Err(e) = tokio::fs::remove_file(entry.path()).await {
                warn!("prune_cache: failed to remove {:?}: {e}", entry.path());
            } else {
                pruned += 1;
            }
        }
    }

    // Full-size clipboard images are named `{hash[..16]}.png` and are the file
    // a `file_image` clip's Content points at. Nothing else removes them when
    // retention deletes the row, so without this sweep they grow forever.
    if let Ok(entries) = std::fs::read_dir(images_dir) {
        for entry in entries.filter_map(|e| e.ok()) {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            let stem = name.rsplit_once('.').map(|(s, _)| s).unwrap_or(&name);
            if known_prefixes.contains(stem) || is_recent(&entry, cutoff) {
                continue;
            }
            if let Err(e) = tokio::fs::remove_file(entry.path()).await {
                warn!("prune_cache: failed to remove {:?}: {e}", entry.path());
            } else {
                pruned += 1;
            }
        }
    }

    // Favicons are keyed by Link-clip domain; titles by URL hash.
    if let Ok(entries) = std::fs::read_dir(favicons_dir) {
        for entry in entries.filter_map(|e| e.ok()) {
            let fname = entry.file_name();
            let name = fname.to_string_lossy();
            if !favicon_is_orphan(&name, &known_favicon_keys) || is_recent(&entry, cutoff) {
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

#[cfg(test)]
mod tests {
    use super::favicon_is_orphan;
    use std::collections::HashSet;

    /// `known` holds link domains plus 16-char URL-hash stems.
    fn known(keys: &[&str]) -> HashSet<String> {
        keys.iter().map(|d| d.to_string()).collect()
    }

    #[test]
    fn keeps_favicons_for_live_link_keys() {
        let set = known(&["github.com", "example.com", "0123456789abcdef"]);
        assert!(!favicon_is_orphan("github.com.webp", &set));
        assert!(!favicon_is_orphan("github.com.dark.webp", &set));
        assert!(
            !favicon_is_orphan("0123456789abcdef.title", &set),
            "title files are keyed by the first 16 chars of the URL hash"
        );
    }

    #[test]
    fn prunes_favicons_whose_key_is_gone() {
        let set = known(&["github.com"]);
        assert!(favicon_is_orphan("example.com.webp", &set));
        assert!(favicon_is_orphan("example.com.dark.webp", &set));
        assert!(favicon_is_orphan("0123456789abcdef.title", &set));
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
