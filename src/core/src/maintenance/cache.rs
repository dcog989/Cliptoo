//! On-disk cache pruning: thumbnail and favicon files with no matching DB
//! record.

use anyhow::Result;
use rusqlite::Connection;
use std::collections::HashSet;
use std::path::Path;
use std::sync::Arc;
use tracing::warn;

use crate::db::DbPool;
use crate::image::HASH_FILENAME_PREFIX_LEN;

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

#[cfg(test)]
mod tests {
    use super::{extract_domain, favicon_is_orphan};
    use std::collections::HashSet;

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
