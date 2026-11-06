//! On-disk cache pruning: thumbnail and favicon files with no matching DB
//! record.

use anyhow::Result;
use rusqlite::{Connection, params};
use std::collections::HashSet;
use std::os::unix::fs::MetadataExt;
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, SystemTime};
use tracing::warn;

use crate::db::DbPool;
use crate::db::models::ClipType;
use crate::image::HASH_FILENAME_PREFIX_LEN;
use crate::url::url_cache_key;

/// Files touched since (pass start - this slack) are left alone. An image
/// capture writes its PNG and thumbnails *before* it inserts the row, so a
/// file created while this pass runs has no DB row yet and must not be
/// deleted; the next pass cleans it if it stays orphaned.
const RECENT_FILE_SLACK: Duration = Duration::from_secs(60);

/// Collect the cache keys still referenced by a stored Link clip: the
/// [`url_cache_key`], which names both the link's favicon file and its
/// page-title file (see `ui/src/favicon.rs`).
fn known_favicon_keys(conn: &Connection) -> Result<HashSet<String>> {
    let mut stmt = conn.prepare_cached("SELECT Content FROM clips WHERE ClipType = ?1")?;
    let mut keys = HashSet::new();
    for row in stmt.query_map(params![ClipType::Link.as_str()], |row| {
        row.get::<_, Option<String>>(0)
    })? {
        match row {
            Ok(Some(content)) => {
                if let Some(key) = url_cache_key(&content) {
                    keys.insert(key);
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
/// Cache files are `{key}.webp`, `{key}.dark.webp` (dark-theme variant) and
/// `{key}.title` (cached page title), all keyed by the URL hash. A file is
/// orphaned when its key appears in no stored Link clip. Files with
/// unrecognised extensions are treated as orphans — the directory is fully
/// managed by the favicon cache.
fn favicon_is_orphan(name: &str, known: &HashSet<String>) -> bool {
    let stem = name
        .strip_suffix(".dark.webp")
        .or_else(|| name.strip_suffix(".webp"))
        .or_else(|| name.strip_suffix(".title"));
    match stem {
        Some(stem) => !known.contains(stem),
        None => true,
    }
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
/// `ContentHash`; favicon and title files by the link's [`url_cache_key`] —
/// each dir is matched against its own key, so a favicon for a still-present
/// link is never pruned. Icons shared by several pages live once in a
/// subdirectory and are hardlinked into those page files.
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

    // Favicons and titles are keyed by the URL hash of a stored Link clip. A
    // subdirectory is the shared icon store: page cache files are hardlinked
    // to its files, so one with a single link is referenced by no page.
    let mut icon_stores = Vec::new();
    if let Ok(entries) = std::fs::read_dir(favicons_dir) {
        for entry in entries.filter_map(|e| e.ok()) {
            if entry.file_type().is_ok_and(|kind| kind.is_dir()) {
                icon_stores.push(entry.path());
                continue;
            }
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
    for store in icon_stores {
        let Ok(entries) = std::fs::read_dir(&store) else {
            continue;
        };
        for entry in entries.filter_map(|e| e.ok()) {
            if is_recent(&entry, cutoff) {
                continue;
            }
            // One link is the store's own entry: no page cache file shares it.
            // A filesystem without hardlink support copies instead, so the
            // entry is dropped and the next page re-downloads the icon.
            if entry.metadata().map(|m| m.nlink()).unwrap_or(1) > 1 {
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

/// Delete every cache file in `thumbnails_dir` and `favicons_dir`, including
/// those belonging to live clips, so the next render or fetch rebuilds them.
/// Bookkeeping dotfiles (e.g. `.preview_ratio`) are kept: dropping the marker
/// would make the next launch wipe the previews this clear is about to
/// rebuild. The favicon dir's shared-icon subdirectory is cache too, so it is
/// removed wholesale. The full-resolution image store is clip data, not cache,
/// and is never touched. Returns the number of files removed.
pub async fn clear_cache_files(thumbnails_dir: &Path, favicons_dir: &Path) -> u64 {
    let mut removed = 0;
    for dir in [thumbnails_dir, favicons_dir] {
        let Ok(mut entries) = tokio::fs::read_dir(dir).await else {
            continue;
        };
        while let Ok(Some(entry)) = entries.next_entry().await {
            let is_dir = entry
                .file_type()
                .await
                .map(|kind| kind.is_dir())
                .unwrap_or(false);
            if is_dir {
                if dir == favicons_dir {
                    if let Ok(mut inner) = tokio::fs::read_dir(entry.path()).await {
                        while let Ok(Some(_)) = inner.next_entry().await {
                            removed += 1;
                        }
                    }
                    let _ = tokio::fs::remove_dir_all(entry.path()).await;
                }
                continue;
            }
            if entry.file_name().to_string_lossy().starts_with('.') {
                continue;
            }
            if tokio::fs::remove_file(entry.path()).await.is_ok() {
                removed += 1;
            }
        }
    }
    removed
}

#[cfg(test)]
mod tests {
    use super::{clear_cache_files, favicon_is_orphan};
    use std::collections::HashSet;

    /// `known` holds the 16-char URL-hash stems of stored Link clips.
    fn known(keys: &[&str]) -> HashSet<String> {
        keys.iter().map(|d| d.to_string()).collect()
    }

    #[test]
    fn keeps_favicons_and_titles_for_live_link_keys() {
        let set = known(&["0123456789abcdef"]);
        assert!(!favicon_is_orphan("0123456789abcdef.webp", &set));
        assert!(!favicon_is_orphan("0123456789abcdef.dark.webp", &set));
        assert!(!favicon_is_orphan("0123456789abcdef.title", &set));
    }

    #[test]
    fn prunes_files_whose_key_is_gone() {
        let set = known(&["0123456789abcdef"]);
        assert!(favicon_is_orphan("fedcba9876543210.webp", &set));
        assert!(favicon_is_orphan("fedcba9876543210.dark.webp", &set));
        assert!(favicon_is_orphan("fedcba9876543210.title", &set));
        assert!(
            favicon_is_orphan("0123456789abcdef", &set),
            "bare name is not a cache file"
        );
        assert!(
            favicon_is_orphan("notes.txt", &set),
            "unknown extensions are orphans"
        );
    }

    #[tokio::test]
    async fn clear_cache_files_removes_live_and_orphan_files_but_keeps_bookkeeping() {
        let root = std::env::temp_dir().join(format!("cliptoo_clear_cache_{}", std::process::id()));
        let td = root.join("thumbnails");
        let fd = root.join("favicons");
        std::fs::create_dir_all(td.join("nested")).unwrap();
        std::fs::create_dir_all(&fd).unwrap();
        for name in [
            "0123456789abcdef.webp",
            "0123456789abcdef_preview.webp",
            ".preview_ratio",
        ] {
            std::fs::write(td.join(name), b"x").unwrap();
        }
        std::fs::write(fd.join("example.com.webp"), b"x").unwrap();
        std::fs::write(fd.join("0123456789abcdef.title"), b"x").unwrap();
        std::fs::create_dir_all(fd.join(".icons")).unwrap();
        std::fs::write(fd.join(".icons/1111111111111111.webp"), b"x").unwrap();

        let removed = clear_cache_files(&td, &fd).await;

        assert_eq!(removed, 5);
        assert!(
            td.join(".preview_ratio").is_file(),
            "the ratio marker must survive"
        );
        assert!(
            td.join("nested").is_dir(),
            "subdirectories are not cache files"
        );
        assert!(
            !fd.join(".icons").exists(),
            "the shared icon store is cache and must be cleared"
        );
        assert!(!td.join("0123456789abcdef.webp").exists());
        assert!(!fd.join("example.com.webp").exists());
        assert!(!fd.join("0123456789abcdef.title").exists());

        let _ = std::fs::remove_dir_all(&root);
    }
}
