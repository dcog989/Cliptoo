use std::collections::{HashMap, HashSet};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

use cliptoo_core::db::DbPool;
use cliptoo_core::favicon_failures;
use cliptoo_core::url::extract_domain;
use futures::StreamExt;
use slint::ComponentHandle;
use slint::Model;

use crate::helpers::{USER_AGENT, is_fetchable_url, read_body_capped};

const FAVICON_FETCH_TIMEOUT_SECS: u64 = 3;

/// Cached icons are drawn at ~16 logical px; 32 keeps them sharp on HiDPI and
/// stops a site's 512px icon from holding a 1 MiB RGBA buffer in the LRU.
const FAVICON_MAX_DIM: u32 = 32;

/// One client for every page and icon fetch. Each `Client` owns a connection
/// pool, so building one per request paid a fresh TLS handshake per icon.
/// Redirects are re-checked: a public URL can otherwise bounce the fetch to a
/// private address that `is_fetchable_url` refused on the first hop.
fn http_client() -> Option<&'static reqwest::Client> {
    static CLIENT: OnceLock<Option<reqwest::Client>> = OnceLock::new();
    CLIENT
        .get_or_init(|| {
            reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(FAVICON_FETCH_TIMEOUT_SECS))
                .user_agent(USER_AGENT)
                .redirect(reqwest::redirect::Policy::custom(|attempt| {
                    if is_fetchable_url(attempt.url().as_str()) {
                        attempt.follow()
                    } else {
                        attempt.stop()
                    }
                }))
                .build()
                .ok()
        })
        .as_ref()
}

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
    if let Some(path) = cached_favicon(&fav_path) {
        return Some(path);
    }
    let client = http_client()?;
    // The page is fetched at most once per call; its own icon declarations are
    // all read from this one response.
    let html = fetch_page(client, url).await;
    store_favicon(client, url, &fav_path, dark, html.as_deref()).await
}

/// Fetch a link's page title and favicon from a single page download. Both
/// live in the same HTML, so this is what keeps one hover from downloading the
/// same page two or three times.
pub async fn fetch_link_meta(
    url: &str,
    fav_dir: &Path,
    dark: bool,
) -> (Option<String>, Option<PathBuf>) {
    let Some(client) = http_client() else {
        return (None, None);
    };
    let html = fetch_page(client, url).await;
    let title = html.as_deref().and_then(page_title);
    let favicon = match extract_domain(url) {
        Some(domain) => {
            let fav_path = fav_dir.join(favicon_file_name(&domain, dark));
            match cached_favicon(&fav_path) {
                Some(path) => Some(path),
                None => store_favicon(client, url, &fav_path, dark, html.as_deref()).await,
            }
        }
        None => None,
    };
    (title, favicon)
}

/// The cached favicon, when the file still starts with a WebP header. A
/// truncated or foreign file is removed so the next fetch replaces it. The
/// header check avoids decoding the whole icon just to validate it.
fn cached_favicon(fav_path: &Path) -> Option<PathBuf> {
    let mut header = [0u8; 12];
    let mut file = std::fs::File::open(fav_path).ok()?;
    if file.read_exact(&mut header).is_ok() && &header[..4] == b"RIFF" && &header[8..] == b"WEBP" {
        return Some(fav_path.to_path_buf());
    }
    let _ = std::fs::remove_file(fav_path);
    None
}

/// Write the best available icon: the site's dark variant, its normal icon,
/// then the DuckDuckGo proxy.
async fn store_favicon(
    client: &reqwest::Client,
    url: &str,
    fav_path: &Path,
    dark: bool,
    html: Option<&str>,
) -> Option<PathBuf> {
    if dark
        && let Some(html) = html
        && let Some(bytes) = fetch_dark_favicon(client, url, html).await
        && save_favicon(fav_path, &bytes)
    {
        return Some(fav_path.to_path_buf());
    }
    if let Some(html) = html
        && let Some(bytes) = fetch_site_favicon(client, url, html).await
        && save_favicon(fav_path, &bytes)
    {
        return Some(fav_path.to_path_buf());
    }
    let domain = extract_domain(url)?;
    let fallback_url = format!("https://icons.duckduckgo.com/ip3/{domain}.ico");
    if let Some(bytes) = download_bytes(client, &fallback_url).await
        && save_favicon(fav_path, &bytes)
    {
        return Some(fav_path.to_path_buf());
    }
    None
}

/// Download a page for its title and icon declarations. Local and private
/// addresses are refused: a copied link can point at the user's own machine.
async fn fetch_page(client: &reqwest::Client, url: &str) -> Option<String> {
    if !is_fetchable_url(url) {
        return None;
    }
    let resp = client.get(url).send().await.ok()?;
    if !resp.status().is_success() {
        return None;
    }
    let bytes = read_body_capped(resp).await?;
    Some(String::from_utf8_lossy(&bytes).into_owned())
}

/// The page's `<title>`, trimmed to one line and with HTML entities decoded
/// (`Fish &amp; Chips` → `Fish & Chips`), so the popup shows page text, not
/// page source.
fn page_title(html: &str) -> Option<String> {
    static TITLE_RE: OnceLock<Option<regex::Regex>> = OnceLock::new();
    let re = TITLE_RE.get_or_init(|| regex::Regex::new(r"(?i)<title>([^<]+)</title>").ok());
    re.as_ref()?
        .captures(html)
        .and_then(|c| c.get(1))
        .map(|m| {
            cliptoo_core::content::decode_entities(m.as_str())
                .trim()
                .to_string()
        })
        .filter(|title| !title.is_empty())
}

/// Regexes for scanning a page for its dark-mode favicon declaration. Two
/// mechanisms exist in the wild: a `<link rel="icon" media="(prefers-color-scheme:
/// dark)">` tag (what a dark browser tab would pick), and a `-dark`-suffixed
/// sibling next to a declared icon (GitHub's `data-base-href` names its icon
/// without an extension, so the suffix is appended to that). Attribute order
/// varies, so each `<link>` tag is captured whole and its attributes inspected
/// individually.
const LINK_TAG_RE: &str = r"(?is)<link\b[^>]*>";
const LINK_ATTR_RE: &str = r#"(?is)\b(rel|media|href|data-base-href)\s*=\s*["']([^"']*)["']"#;

/// Extensions tried for a `-dark` sibling, matching the common asset naming.
const DARK_PROBE_EXTENSIONS: [&str; 3] = ["png", "webp", "svg"];

/// Extensions stripped from a declared icon href before the `-dark` suffix is
/// inserted (`favicon.svg` → `favicon-dark.svg`).
const ICON_EXTENSIONS: [&str; 8] = ["avif", "gif", "ico", "jpeg", "jpg", "png", "svg", "webp"];

/// Every declared icon costs one probe per extension. Cap the fan-out so a
/// page declaring many sizes does not fire dozens of requests on one hover.
const MAX_DARK_FAVICON_PROBES: usize = 12;

/// Dark-variant favicon URL candidates from a already-downloaded page, as
/// `(direct, probes)`. `direct` hrefs are declared dark-mode icons; `probes`
/// are `-dark` siblings that only exist if they return 200.
type DarkFaviconCandidates = (Vec<String>, Vec<String>);

/// Pick the dark-mode favicon out of the page's `<link>` declarations and
/// `-dark` siblings.
async fn fetch_dark_favicon(
    client: &reqwest::Client,
    base_url: &str,
    html: &str,
) -> Option<Vec<u8>> {
    let (direct, probes) = find_dark_favicon_candidates(html, base_url);
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
        let is_icon = rel
            .as_deref()
            .is_some_and(|r| r.split_whitespace().any(|w| w == "icon"));
        let dark_media = media.as_deref().is_some_and(|m| m.contains("dark"));
        let declared = href.as_deref().filter(|href| !href.starts_with("data:"));
        if is_icon
            && dark_media
            && let Some(url) = declared.and_then(|href| resolve_url(base_url, href))
        {
            direct.push(url);
        }
        // GitHub puts `data-base-href` on a non-icon `<link>`, so this probe
        // must not be gated on the `rel` attribute. A declared dark icon is
        // already in `direct`, so only the non-dark declarations are probed.
        let base_source = base.as_deref().filter(|href| !href.starts_with("data:"));
        let icon_source = if is_icon && !dark_media {
            declared
        } else {
            None
        };
        for source in base_source.into_iter().chain(icon_source) {
            if let Some(url) = resolve_url(base_url, source) {
                probes.extend(dark_probe_urls(&url));
            }
        }
    }
    probes.sort_unstable();
    probes.dedup();
    probes.truncate(MAX_DARK_FAVICON_PROBES);
    (direct, probes)
}

/// `-dark` siblings for one icon URL: `favicon.svg` → `favicon-dark.{png,webp,svg}`.
/// Query and fragment are dropped because the sibling is a different file, and
/// a URL that already names a dark asset is left alone.
fn dark_probe_urls(url: &str) -> Vec<String> {
    let stem = icon_stem(url);
    if stem.ends_with("-dark") {
        return Vec::new();
    }
    DARK_PROBE_EXTENSIONS
        .iter()
        .map(|ext| format!("{stem}-dark.{ext}"))
        .collect()
}

/// Strip one recognised image extension from a URL's path, leaving any
/// extensionless asset name (GitHub's `.../favicon`) untouched.
fn icon_stem(url: &str) -> &str {
    let clean = url.split(['?', '#']).next().unwrap_or(url);
    let Some((stem, ext)) = clean.rsplit_once('.') else {
        return clean;
    };
    if ICON_EXTENSIONS
        .iter()
        .any(|known| ext.eq_ignore_ascii_case(known))
    {
        stem
    } else {
        clean
    }
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
    static TAG_RE: OnceLock<Option<regex::Regex>> = OnceLock::new();
    static ATTR_RE: OnceLock<Option<regex::Regex>> = OnceLock::new();
    let tag_re = TAG_RE.get_or_init(|| regex::Regex::new(LINK_TAG_RE).ok());
    let attr_re = ATTR_RE.get_or_init(|| regex::Regex::new(LINK_ATTR_RE).ok());
    let (Some(tag_re), Some(attr_re)) = (tag_re.as_ref(), attr_re.as_ref()) else {
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
async fn fetch_site_favicon(
    client: &reqwest::Client,
    base_url: &str,
    html: &str,
) -> Option<Vec<u8>> {
    // The spec's default icon is the last `rel="icon"` without a `sizes`
    // attribute; try from the end so an overriding declaration wins, then work
    // backwards through the rest.
    for url in site_favicon_hrefs(html, base_url).into_iter().rev() {
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
    if !is_fetchable_url(url) {
        return None;
    }
    let resp = client.get(url).send().await.ok()?;
    if !resp.status().is_success() {
        return None;
    }
    read_body_capped(resp).await
}

/// Persist favicon `bytes` to `fav_path` as WebP, downscaled to
/// [`FAVICON_MAX_DIM`]. Bitmap formats go through the `image` crate; SVG is
/// rasterized with resvg first. Written through a temp file so a crash cannot
/// leave a truncated icon that later calls treat as cached.
fn save_favicon(fav_path: &Path, bytes: &[u8]) -> bool {
    if let Some(parent) = fav_path.parent()
        && std::fs::create_dir_all(parent).is_err()
    {
        return false;
    }
    if let Some(img) = cliptoo_core::image::decode_raster(bytes) {
        let icon = cliptoo_core::image::resize_to(&img, FAVICON_MAX_DIM);
        return cliptoo_core::image::save_atomic(&icon, fav_path, image::ImageFormat::WebP).is_ok();
    }
    if looks_like_svg(bytes)
        && let Ok((rgba, w, h)) = cliptoo_core::icon::rasterize_svg(bytes, FAVICON_MAX_DIM)
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
        return image::RgbaImage::from_raw(w, h, straight).is_some_and(|img| {
            let icon = image::DynamicImage::ImageRgba8(img);
            cliptoo_core::image::save_atomic(&icon, fav_path, image::ImageFormat::WebP).is_ok()
        });
    }
    false
}

fn looks_like_svg(bytes: &[u8]) -> bool {
    std::str::from_utf8(bytes)
        .ok()
        .map(str::trim_start)
        .is_some_and(|s| s.starts_with("<svg") || s.starts_with("<?xml"))
}

/// Prefix length of the URL hash used as the title cache key. Shared with the
/// maintenance sweep, which must recognise live title files as live.
const TITLE_HASH_PREFIX_LEN: usize = cliptoo_core::image::HASH_FILENAME_PREFIX_LEN;

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
///
/// The ledger itself lives in `cliptoo_core::favicon_failures` so the
/// maintenance pass can prune counts for clips it deletes.
///
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
    // Loaded on first use: a list whose links all have icons never pays for it.
    let mut failures: Option<HashMap<i64, u32>> = None;
    for i in 0..model.row_count() {
        if let Some(data) = model.row_data(i) {
            let ct = data.clip_type.as_str();
            if ct == "link" && data.favicon_image.size().width == 0 {
                // Budget exhausted: never fetch this clip again (until caches
                // are cleared) — the favicon has proven unobtainable.
                let failures = failures.get_or_insert_with(|| favicon_failures::load(favicons_dir));
                if favicon_failures::exhausted(failures, data.id as i64) {
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
                let domain = extract_domain(&content);
                let fetched = fetch_favicon(&content, &fav_dir, dark).await.is_some();
                Some((clip_id, domain, fetched))
            }
        }))
        .buffer_unordered(FAVICON_FETCH_CONCURRENCY)
        .collect::<Vec<_>>()
        .await;

        let mut outcomes = Vec::with_capacity(results.len());
        let mut reloaded_domains = HashSet::new();
        for (clip_id, domain, fetched) in results.into_iter().flatten() {
            outcomes.push((clip_id, fetched));
            // Remember the domains whose icon arrived: only rows showing that
            // icon are reloaded, not the whole list.
            if fetched && let Some(domain) = domain {
                reloaded_domains.insert(domain);
            }
        }
        // One read-modify-write for the whole batch; a per-clip update would be
        // O(clips²) file writes.
        favicon_failures::record(&fav_dir, &outcomes);
        if !reloaded_domains.is_empty() {
            let _ = weak.upgrade_in_event_loop(move |ui| {
                crate::thumbnail_cache::reload_favicons_for(&ui, &fav_dir, &reloaded_domains);
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
    fn extracts_the_page_title() {
        assert_eq!(
            page_title("<html><head><title> Hello &amp; bye </title></head></html>"),
            Some("Hello & bye".to_string())
        );
        assert_eq!(
            page_title("<title>caf&#233;</title>"),
            Some("café".to_string())
        );
        assert_eq!(page_title("<title></title>"), None);
        assert_eq!(page_title("<p>no title</p>"), None);
    }

    #[test]
    fn probes_dark_icon_from_a_non_icon_base_href() {
        // GitHub carries `data-base-href` on a stylesheet link, not an icon one;
        // the probe must still be generated.
        let html =
            r#"<link data-base-href="https://github.com/foo/bar" rel="stylesheet" href="x.css">"#;
        let (direct, probes) = find_dark_favicon_candidates(html, "https://github.com/");
        assert!(direct.is_empty());
        assert_eq!(
            probes.first().map(String::as_str),
            Some("https://github.com/foo/bar-dark.png")
        );
    }

    #[test]
    fn probes_dark_siblings_of_declared_icons() {
        let html = r#"
            <link rel="icon" href="/favicon.png">
            <link rel="icon" href="/favicon.svg?v=2">
            <link rel="icon" href="/favicon">
            <link rel="icon" href="/icon-dark.png">
            <link rel="icon" media="(prefers-color-scheme: dark)" href="/dark.svg">
            <link rel="apple-touch-icon" href="/touch.png">
        "#;
        let (direct, probes) = find_dark_favicon_candidates(html, "https://example.com/page");
        assert_eq!(direct, vec!["https://example.com/dark.svg".to_string()]);
        assert_eq!(
            probes,
            vec![
                "https://example.com/favicon-dark.png".to_string(),
                "https://example.com/favicon-dark.svg".to_string(),
                "https://example.com/favicon-dark.webp".to_string(),
            ]
        );
    }
}
