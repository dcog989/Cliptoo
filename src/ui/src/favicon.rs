use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

use crate::helpers::extract_domain;
use cliptoo_core::db::DbPool;
use futures::StreamExt;
use slint::ComponentHandle;
use slint::Model;

const FAVICON_FETCH_TIMEOUT_SECS: u64 = 3;

/// Domains fetched at once. Every fetch can make up to three HTTP requests, so
/// this keeps a large link history from opening hundreds of connections.
const FAVICON_FETCH_CONCURRENCY: usize = 4;

/// Cache-file suffixes for the two theme variants. Light and dark favicons
/// are cached side-by-side (`{domain}.webp` vs `{domain}.dark.webp`) so
/// switching themes never evicts the other variant.
pub const FAVICON_LIGHT_SUFFIX: &str = ".webp";
pub const FAVICON_DARK_SUFFIX: &str = ".dark.webp";

/// The cache filename for a domain's favicon under the given theme.
pub fn favicon_file_name(domain: &str, dark: bool) -> String {
    if dark {
        format!("{domain}{FAVICON_DARK_SUFFIX}")
    } else {
        format!("{domain}{FAVICON_LIGHT_SUFFIX}")
    }
}

/// Fetch a link's favicon, honouring the active theme. In dark mode the
/// site's own `prefers-color-scheme: dark` favicon is used when one is
/// declared (matching what a browser tab would show), falling back to the
/// DuckDuckGo icon so sites without a dark variant still get a cached icon.
pub async fn fetch_favicon(url: &str, fav_dir: &Path, dark: bool) -> Option<PathBuf> {
    let domain = extract_domain(url)?;
    let fav_path = fav_dir.join(favicon_file_name(&domain, dark));
    if fav_path.exists() {
        let bytes = std::fs::read(&fav_path).ok()?;
        if image::load_from_memory(&bytes).is_ok() {
            return Some(fav_path);
        }
        let _ = std::fs::remove_file(&fav_path);
    }
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(FAVICON_FETCH_TIMEOUT_SECS))
        .user_agent(crate::helpers::USER_AGENT)
        .build()
        .ok()?;
    if dark
        && let Some(bytes) = fetch_dark_favicon(&client, url).await
        && save_favicon(&fav_path, &bytes)
    {
        return Some(fav_path);
    }
    // The site's own (non-dark) declared icon, when it has one. Some sites
    // ship only a normal favicon (no dark variant) that the DuckDuckGo proxy
    // doesn't know about, so without this the row would stay icon-less forever.
    if let Some(bytes) = fetch_site_favicon(&client, url).await
        && save_favicon(&fav_path, &bytes)
    {
        return Some(fav_path);
    }
    let fallback_url = format!("https://icons.duckduckgo.com/ip3/{domain}.ico");
    if let Some(bytes) = download_bytes(&client, &fallback_url).await
        && save_favicon(&fav_path, &bytes)
    {
        return Some(fav_path);
    }
    None
}

/// Regexes for scanning a page for its dark-mode favicon declaration. Two
/// mechanisms exist in the wild: a `<link rel="icon" media="(prefers-color-scheme:
/// dark)">` tag (what a dark browser tab would pick), and GitHub's
/// `data-base-href` attribute, where a `-dark`-suffixed sibling is swapped in
/// by client-side JS. Attribute order varies, so each `<link>` tag is captured
/// whole and its attributes inspected individually.
const LINK_TAG_RE: &str = r"(?is)<link\b[^>]*>";
const LINK_ATTR_RE: &str = r#"(?is)\b(rel|media|href|data-base-href)\s*=\s*["']([^"']*)["']"#;

/// Dark-variant favicon URL candidates from a page, as `(direct, probes)`.
/// `direct` hrefs are declared dark-mode icons; `probes` are GitHub-style
/// `-dark` siblings that only exist if they return 200.
type DarkFaviconCandidates = (Vec<String>, Vec<String>);

/// Fetch the page at `base_url` and download its dark-mode favicon, if it has
/// one. Tries declared `prefers-color-scheme: dark` icons first, then probes
/// GitHub-style `{base}-dark.{png,webp,svg}` siblings.
async fn fetch_dark_favicon(client: &reqwest::Client, base_url: &str) -> Option<Vec<u8>> {
    let resp = client.get(base_url).send().await.ok()?;
    if !resp.status().is_success() {
        return None;
    }
    let html = resp.text().await.ok()?;
    let (direct, probes) = find_dark_favicon_candidates(&html, base_url);
    for url in direct.into_iter().chain(probes) {
        if let Some(bytes) = download_bytes(client, &url).await {
            return Some(bytes);
        }
    }
    None
}

fn find_dark_favicon_candidates(html: &str, base_url: &str) -> DarkFaviconCandidates {
    let mut direct = Vec::new();
    let mut probes = Vec::new();
    for (rel, media, href, base) in parse_link_tags(html) {
        let Some(rel) = rel else {
            continue;
        };
        if !rel.split_whitespace().any(|w| w == "icon") {
            continue;
        }
        if let Some(media) = media.as_deref()
            && media.contains("dark")
            && let Some(href) = href.as_deref()
            && !href.starts_with("data:")
            && let Some(url) = resolve_url(base_url, href)
        {
            direct.push(url);
        }
        if let Some(base) = base.as_deref()
            && let Some(resolved) = resolve_url(base_url, base)
        {
            for ext in ["png", "webp", "svg"] {
                probes.push(format!("{resolved}-dark.{ext}"));
            }
        }
    }
    (direct, probes)
}

/// Parse every `<link>` tag in `html` into its `(rel, media, href,
/// data-base-href)` attributes. Attribute order varies in the wild, so each tag
/// is captured whole and its attributes inspected individually. Shared by the
/// dark-variant probe and the generic site-favicon fallback.
type LinkAttrs = (
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
);

fn parse_link_tags(html: &str) -> Vec<LinkAttrs> {
    let tag_re = regex::Regex::new(LINK_TAG_RE).ok();
    let attr_re = regex::Regex::new(LINK_ATTR_RE).ok();
    let (Some(tag_re), Some(attr_re)) = (tag_re, attr_re) else {
        return Vec::new();
    };
    let mut tags = Vec::new();
    for cap in tag_re.captures_iter(html) {
        let Some(tag) = cap.get(0) else {
            continue;
        };
        let mut rel = None;
        let mut media = None;
        let mut href = None;
        let mut base = None;
        for a in attr_re.captures_iter(tag.as_str()) {
            match &a[1] {
                "rel" => rel = Some(a[2].to_ascii_lowercase()),
                "media" => media = Some(a[2].to_ascii_lowercase()),
                "href" => href = Some(a[2].to_string()),
                "data-base-href" => base = Some(a[2].to_string()),
                _ => {}
            }
        }
        tags.push((rel, media, href, base));
    }
    tags
}

/// Fetch the site's own declared favicon — any `rel="icon"` link, not gated on
/// a dark-mode `media` query. Runs after the dark-variant probe so a dark icon
/// is preferred in dark themes, but a site that only ships a normal icon still
/// gets its favicon instead of falling through to the DuckDuckGo proxy (which
/// 404s for many smaller sites).
async fn fetch_site_favicon(client: &reqwest::Client, base_url: &str) -> Option<Vec<u8>> {
    let resp = client.get(base_url).send().await.ok()?;
    if !resp.status().is_success() {
        return None;
    }
    let html = resp.text().await.ok()?;
    // The spec's default icon is the last `rel="icon"` without a `sizes`
    // attribute; try from the end so an overriding declaration wins, then work
    // backwards through the rest.
    for url in site_favicon_hrefs(&html, base_url).into_iter().rev() {
        if let Some(bytes) = download_bytes(client, &url).await {
            return Some(bytes);
        }
    }
    None
}

/// Collect the site's non-dark declared icon URLs, in document order.
fn site_favicon_hrefs(html: &str, base_url: &str) -> Vec<String> {
    let mut hrefs = Vec::new();
    for (rel, media, href, _) in parse_link_tags(html) {
        let Some(rel) = rel else {
            continue;
        };
        if !rel.split_whitespace().any(|w| w == "icon") {
            continue;
        }
        // Dark-mode variants are probed separately and take priority.
        if media.as_deref().is_some_and(|m| m.contains("dark")) {
            continue;
        }
        let Some(href) = href.as_deref() else {
            continue;
        };
        if href.starts_with("data:") {
            continue;
        }
        if let Some(url) = resolve_url(base_url, href) {
            hrefs.push(url);
        }
    }
    hrefs
}

/// Resolve a (possibly relative) favicon `href` against the page's base URL.
/// RFC 3986 resolution via `url::Url::join`, so a relative icon declared by a
/// page under a path (e.g. `/page/favicon.svg`) resolves against that page,
/// while a root-relative href (`/icon.png`) resolves against the origin.
fn resolve_url(base_url: &str, href: &str) -> Option<String> {
    url::Url::parse(base_url)
        .ok()?
        .join(href)
        .ok()
        .map(|u| u.to_string())
}

async fn download_bytes(client: &reqwest::Client, url: &str) -> Option<Vec<u8>> {
    let resp = client.get(url).send().await.ok()?;
    if !resp.status().is_success() {
        return None;
    }
    resp.bytes().await.ok().map(|b| b.to_vec())
}

/// Persist favicon `bytes` to `fav_path` as WebP. Bitmap formats go through
/// the `image` crate; SVG is rasterized with resvg first.
fn save_favicon(fav_path: &Path, bytes: &[u8]) -> bool {
    if let Some(parent) = fav_path.parent()
        && std::fs::create_dir_all(parent).is_err()
    {
        return false;
    }
    if let Ok(img) = image::load_from_memory(bytes) {
        return img.save(fav_path).is_ok();
    }
    if looks_like_svg(bytes)
        && let Ok((rgba, w, h)) = cliptoo_core::icon::rasterize_svg(bytes, 32)
    {
        // resvg produces premultiplied alpha; convert to straight alpha so
        // the saved WebP renders without halo artifacts.
        let mut straight = rgba;
        for px in straight.chunks_mut(4) {
            let a = px[3] as u32;
            if a > 0 && a < 255 {
                px[0] = ((px[0] as u32 * 255) / a) as u8;
                px[1] = ((px[1] as u32 * 255) / a) as u8;
                px[2] = ((px[2] as u32 * 255) / a) as u8;
            }
        }
        return image::RgbaImage::from_raw(w, h, straight)
            .is_some_and(|img| img.save(fav_path).is_ok());
    }
    false
}

fn looks_like_svg(bytes: &[u8]) -> bool {
    std::str::from_utf8(bytes)
        .ok()
        .map(str::trim_start)
        .is_some_and(|s| s.starts_with("<svg") || s.starts_with("<?xml"))
}

/// Prefix length of the URL hash used as the title cache key.
const TITLE_HASH_PREFIX_LEN: usize = 16;

/// Title cache file for one page. Keyed by the whole URL, not the domain: two
/// pages on the same site must not share (or overwrite) each other's title.
fn title_cache_path(url: &str, fav_dir: &Path) -> Option<PathBuf> {
    extract_domain(url)?;
    let (hash, _) = cliptoo_core::content::hash::sha256_hex_and_prefix(url);
    Some(fav_dir.join(format!("{}.title", &hash[..TITLE_HASH_PREFIX_LEN])))
}

pub fn load_cached_page_title(url: &str, fav_dir: &Path) -> Option<String> {
    let path = title_cache_path(url, fav_dir)?;
    std::fs::read_to_string(&path).ok()
}

pub fn cache_page_title(url: &str, title: &str, fav_dir: &Path) {
    let Some(path) = title_cache_path(url, fav_dir) else {
        return;
    };
    let _ = std::fs::write(&path, title);
}

/// Domains for which a favicon fetch has already been dispatched this session.
/// Inserted at dispatch time so `check_pending_favicons` neither double-fetches
/// an in-flight domain across overlapping refreshes nor re-fetches a domain
/// whose favicon cannot be obtained (no site icon + DuckDuckGo 404) on every
/// refresh. A successful fetch reloads all matching rows, so no removal is
/// needed. Per-session only: cleared on restart, so a site that adds a favicon
/// later is retried.
fn attempted_favicon_domains() -> &'static Mutex<HashSet<String>> {
    static ATTEMPTED: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();
    ATTEMPTED.get_or_init(|| Mutex::new(HashSet::new()))
}

/// A clip stops being fetched after this many failed favicon attempts. The
/// ledger is persistent and only reset by the "Clear caches" maintenance
/// action, so a domain with no obtainable favicon costs at most a handful of
/// requests ever instead of one per session forever.
const MAX_FAVICON_FAILURES: u32 = 5;
pub(crate) const FAVICON_FAILURES_FILE: &str = "favicon_failures.txt";

/// Path of the persistent favicon-fetch failure ledger. Lives beside (not
/// inside) the favicon cache dir, because scheduled cache pruning deletes any
/// unrecognised file it finds in `favicons_dir` — which would silently reset
/// the counts on every maintenance run. Only the explicit "Clear caches"
/// action resets the ledger.
fn failures_path(favicons_dir: &Path) -> PathBuf {
    favicons_dir
        .parent()
        .unwrap_or(favicons_dir)
        .join(FAVICON_FAILURES_FILE)
}

/// Read the per-clip favicon fetch-failure counts, as `clip_id → failures`.
fn load_failures(favicons_dir: &Path) -> HashMap<i64, u32> {
    let Ok(raw) = std::fs::read_to_string(failures_path(favicons_dir)) else {
        return HashMap::new();
    };
    raw.lines()
        .filter_map(|line| line.split_once(':'))
        .filter_map(|(id, count)| Some((id.trim().parse().ok()?, count.trim().parse().ok()?)))
        .collect()
}

fn save_failures(favicons_dir: &Path, failures: &HashMap<i64, u32>) {
    let mut ids: Vec<_> = failures.keys().copied().collect();
    ids.sort_unstable();
    let mut out = String::new();
    for id in ids {
        out.push_str(&format!("{id}:{}\n", failures[&id]));
    }
    let _ = std::fs::write(failures_path(favicons_dir), out);
}

/// Serialises the read-modify-write of the failure ledger. Concurrent fetch
/// batches otherwise lose each other's increments.
fn failure_ledger_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

fn increment_failure(favicons_dir: &Path, clip_id: i64) {
    let _guard = failure_ledger_lock()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let mut failures = load_failures(favicons_dir);
    let count = failures.entry(clip_id).or_insert(0);
    *count = count.saturating_add(1).min(MAX_FAVICON_FAILURES);
    save_failures(favicons_dir, &failures);
}

fn reset_failure(favicons_dir: &Path, clip_id: i64) {
    let _guard = failure_ledger_lock()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let mut failures = load_failures(favicons_dir);
    if failures.remove(&clip_id).is_some() {
        save_failures(favicons_dir, &failures);
    }
}

fn failures_exhausted(failures: &HashMap<i64, u32>, clip_id: i64) -> bool {
    failures.get(&clip_id).copied().unwrap_or(0) >= MAX_FAVICON_FAILURES
}

/// Reset every clip's favicon-fetch failure budget. Called by the "Clear
/// caches" maintenance action so a cache clear also re-allows favicon retries.
pub fn reset_favicon_failures(favicons_dir: &Path) {
    let _ = std::fs::remove_file(failures_path(favicons_dir));
}

/// After populating the clip list, scan for link clips without cached
/// favicons and fetch them in the background. The model is reloaded once
/// after the batch finishes.
///
/// Must be called on the UI thread (e.g. inside `upgrade_in_event_loop`)
/// because it accesses the Slint model synchronously to collect pending
/// clip IDs.  The actual HTTP fetching happens on the tokio runtime.
pub fn check_pending_favicons(ui: &crate::AppWindow, db: &Arc<DbPool>, favicons_dir: &Path) {
    let model = ui.get_clips();
    let mut pending = Vec::new();
    let mut attempted = attempted_favicon_domains()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let failures = {
        let _guard = failure_ledger_lock()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        load_failures(favicons_dir)
    };
    for i in 0..model.row_count() {
        if let Some(data) = model.row_data(i) {
            let ct = data.clip_type.as_str();
            if ct == "link" && data.favicon_image.size().width == 0 {
                // Budget exhausted: never fetch this clip again (until caches
                // are cleared) — the favicon has proven unobtainable.
                if failures_exhausted(&failures, data.id as i64) {
                    continue;
                }
                let Some(domain) = extract_domain(&data.preview_content) else {
                    continue;
                };
                if attempted.insert(domain) {
                    pending.push(data.id as i64);
                }
            }
        }
    }
    // Stop holding the dispatch ledger while the HTTP work runs: a refresh
    // during a slow fetch must not block on this mutex.
    drop(attempted);
    if pending.is_empty() {
        return;
    }
    // Fetch the variant matching the current theme (dark cache entries are
    // fetched separately from light ones). One task with bounded concurrency:
    // one spawned task per domain (the old code) opened hundreds of requests at
    // once, and every success reloaded the whole list, for O(domains × rows) UI
    // work.
    let dark = crate::theme::cached_resolved_theme().0;
    let weak = ui.as_weak();
    let db = db.clone();
    let fav_dir = favicons_dir.to_owned();
    tokio::spawn(async move {
        let fetch_dir = fav_dir.clone();
        let results = futures::stream::iter(pending.into_iter().map(move |clip_id| {
            let db = db.clone();
            let fav_dir = fetch_dir.clone();
            async move {
                let content = db
                    .with(|conn| cliptoo_core::db::queries::get_clip_content(conn, clip_id))
                    .await
                    .ok()?;
                let fetched = fetch_favicon(&content, &fav_dir, dark).await.is_some();
                Some((clip_id, fetched))
            }
        }))
        .buffer_unordered(FAVICON_FETCH_CONCURRENCY)
        .collect::<Vec<_>>()
        .await;

        let mut any_fetched = false;
        for (clip_id, fetched) in results.into_iter().flatten() {
            if fetched {
                // Clear this clip's failure count; the single reload below
                // picks up the icon for every row sharing the domain.
                reset_failure(&fav_dir, clip_id);
                any_fetched = true;
            } else {
                // After the budget is exhausted the scan stops dispatching it.
                increment_failure(&fav_dir, clip_id);
            }
        }
        if any_fetched {
            let _ = weak.upgrade_in_event_loop(move |ui| {
                crate::thumbnail_cache::reload_favicons(&ui, &fav_dir);
            });
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_icon_links_regardless_of_attribute_order() {
        let html = r#"
            <link rel="icon" sizes="32x32" href="/favicon.ico">
            <link href="icon.png" media="(prefers-color-scheme: dark)" rel="icon">
            <link rel="shortcut icon" href="//cdn.example.com/icon.svg">
            <link data-base-href="https://github.com/foo/bar" rel="stylesheet" href="style.css">
        "#;
        let tags = parse_link_tags(html);
        assert_eq!(tags.len(), 4);
        // Default icon: rel + href captured in any order.
        assert_eq!(tags[0].0.as_deref(), Some("icon"));
        assert_eq!(tags[0].2.as_deref(), Some("/favicon.ico"));
        // Dark variant.
        assert_eq!(tags[1].1.as_deref(), Some("(prefers-color-scheme: dark)"));
        assert_eq!(tags[1].2.as_deref(), Some("icon.png"));
        // `rel` with extra keywords still counts as an icon.
        assert_eq!(tags[2].0.as_deref(), Some("shortcut icon"));
        assert_eq!(tags[2].2.as_deref(), Some("//cdn.example.com/icon.svg"));
        // Non-icon stylesheet carries the data-base-href.
        assert_eq!(tags[3].3.as_deref(), Some("https://github.com/foo/bar"));
    }

    #[test]
    fn site_favicon_collects_icon_hrefs_and_skips_dark_and_data() {
        let html = r#"
            <link rel="icon" href="data:image/svg+xml,...">
            <link rel="icon" sizes="16x16" href="/favicon-16.png">
            <link rel="icon" media="(prefers-color-scheme: dark)" href="/dark.svg">
            <link rel="icon" href="/favicon-32.png">
        "#;
        assert_eq!(
            site_favicon_hrefs(html, "https://example.com"),
            vec![
                "https://example.com/favicon-16.png".to_string(),
                "https://example.com/favicon-32.png".to_string(),
            ]
        );
    }

    #[test]
    fn titles_are_cached_per_url_not_per_domain() {
        let dir = Path::new("/tmp/cliptoo-titles");
        let first = title_cache_path("https://github.com/a", dir).unwrap();
        let second = title_cache_path("https://github.com/b", dir).unwrap();
        assert_ne!(first, second);
        assert_eq!(
            first,
            title_cache_path("https://github.com/a", dir).unwrap()
        );
    }

    #[test]
    fn resolves_relative_favicon_against_page_path_not_domain_root() {
        // Regression: a page under a path declaring `favicon.svg` used to be
        // resolved against the domain root, 404ing for GitHub Pages sites.
        assert_eq!(
            resolve_url(
                "https://dcog989.github.io/Default-fonts-per-OS/",
                "favicon.svg"
            )
            .unwrap(),
            "https://dcog989.github.io/Default-fonts-per-OS/favicon.svg"
        );
        assert_eq!(
            resolve_url(
                "https://dcog989.github.io/Goat-Color-Picker-Palette/",
                "favicon.svg"
            )
            .unwrap(),
            "https://dcog989.github.io/Goat-Color-Picker-Palette/favicon.svg"
        );
    }

    #[test]
    fn resolves_root_relative_and_absolute_hrefs() {
        assert_eq!(
            resolve_url("https://example.com/dir/page.html", "/root-icon.png").unwrap(),
            "https://example.com/root-icon.png"
        );
        assert_eq!(
            resolve_url(
                "https://example.com/dir/page.html",
                "//cdn.example.com/i.png"
            )
            .unwrap(),
            "https://cdn.example.com/i.png"
        );
        assert_eq!(
            resolve_url(
                "https://example.com/dir/page.html",
                "https://other.net/i.svg"
            )
            .unwrap(),
            "https://other.net/i.svg"
        );
        assert_eq!(
            resolve_url("https://example.com/page?x=1", "icon.png").unwrap(),
            "https://example.com/icon.png"
        );
    }

    #[test]
    fn failure_budget_exhausts_at_limit_and_resets_on_clear_cache() {
        let dir = std::env::temp_dir().join("cliptoo-favicon-failures-test");
        let fav_dir = dir.join("favicons");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&fav_dir).unwrap();

        assert!(!failures_exhausted(&load_failures(&fav_dir), 42));
        for _ in 0..5 {
            increment_failure(&fav_dir, 42);
        }
        assert!(failures_exhausted(&load_failures(&fav_dir), 42));

        // "Clear caches" deletes the ledger, re-allowing fetches.
        reset_favicon_failures(&fav_dir);
        assert!(!failures_exhausted(&load_failures(&fav_dir), 42));

        // A successful fetch clears a single clip's count.
        increment_failure(&fav_dir, 7);
        increment_failure(&fav_dir, 7);
        reset_failure(&fav_dir, 7);
        assert!(!failures_exhausted(&load_failures(&fav_dir), 7));
        assert!(!failures_exhausted(&load_failures(&fav_dir), 42));

        let _ = std::fs::remove_dir_all(&dir);
    }
}
