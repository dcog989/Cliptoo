use std::collections::{HashMap, HashSet};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

use cliptoo_core::db::DbPool;
use cliptoo_core::favicon_failures;
use cliptoo_core::url::{extract_domain, url_cache_key};
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

/// Completed fetches between UI reloads. Icons then land in the list as they
/// arrive instead of all at the end of a long batch, without paying a reload
/// per icon. One wave of [`FAVICON_FETCH_CONCURRENCY`] is enough for the first
/// icons to show up.
const FAVICON_RELOAD_EVERY: usize = FAVICON_FETCH_CONCURRENCY;

/// Cache-file suffixes for the two theme variants. Light and dark favicons
/// are cached side-by-side (`{key}.webp` vs `{key}.dark.webp`) so
/// switching themes never evicts the other variant.
pub const FAVICON_LIGHT_SUFFIX: &str = ".webp";
pub const FAVICON_DARK_SUFFIX: &str = ".dark.webp";

/// The cache filename for a favicon cache key under the given theme.
pub fn favicon_file_name(key: &str, dark: bool) -> String {
    if dark {
        format!("{key}{FAVICON_DARK_SUFFIX}")
    } else {
        format!("{key}{FAVICON_LIGHT_SUFFIX}")
    }
}

/// Shared icon store inside the favicon cache, named by the icon's own URL.
/// A page's cache file is hardlinked to its icon here, so pages that declare
/// the same icon share one download and one copy of the bytes.
const ICON_STORE_DIR: &str = ".icons";

fn icon_store_path(fav_dir: &Path, icon_url: &str) -> Option<PathBuf> {
    let key = url_cache_key(icon_url)?;
    Some(
        fav_dir
            .join(ICON_STORE_DIR)
            .join(favicon_file_name(&key, false)),
    )
}

/// Put `icon_url`'s icon in this page's cache slot. A store hit skips the
/// request entirely: another page on this or any other host already fetched
/// the same icon URL.
async fn fetch_icon(
    client: &reqwest::Client,
    icon_url: &str,
    fav_dir: &Path,
    fav_path: &Path,
) -> bool {
    let Some(store_path) = icon_store_path(fav_dir, icon_url) else {
        return false;
    };
    if cached_favicon(&store_path).is_some() {
        return link_icon(&store_path, fav_path);
    }
    let Some(bytes) = download_bytes(client, icon_url).await else {
        return false;
    };
    save_favicon(&store_path, &bytes) && link_icon(&store_path, fav_path)
}

/// Make this page's cache file share the stored icon's bytes. The cache is a
/// plain directory, so a filesystem without hardlinks only pays a copy.
fn link_icon(store_path: &Path, fav_path: &Path) -> bool {
    let _ = std::fs::remove_file(fav_path);
    std::fs::hard_link(store_path, fav_path)
        .or_else(|_| std::fs::copy(store_path, fav_path).map(|_| ()))
        .is_ok()
}

/// Fetch a link's favicon, honouring the active theme. In dark mode the
/// site's own `prefers-color-scheme: dark` favicon is used when one is
/// declared (matching what a browser tab would show), falling back to the
/// DuckDuckGo icon so sites without a dark variant still get a cached icon.
/// Icons are stored once per icon URL and hardlinked into the page's cache
/// file, so pages that declare the same icon share one download.
pub async fn fetch_favicon(url: &str, fav_dir: &Path, dark: bool) -> Option<PathBuf> {
    let key = url_cache_key(url)?;
    let fav_path = fav_dir.join(favicon_file_name(&key, dark));
    if let Some(path) = cached_favicon(&fav_path) {
        return Some(path);
    }
    let client = http_client()?;
    // The page is fetched at most once per call; its own icon declarations are
    // all read from this one response.
    let html = fetch_page(client, url).await;
    // The page download already carries the title; keep it so a later hover
    // does not fetch the same page again just for the title.
    if let Some(title) = html.as_deref().and_then(page_title) {
        cache_page_title(url, &title, fav_dir);
    }
    store_favicon(client, url, fav_dir, &fav_path, dark, html.as_deref()).await
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
    let favicon = match url_cache_key(url) {
        Some(key) => {
            let fav_path = fav_dir.join(favicon_file_name(&key, dark));
            match cached_favicon(&fav_path) {
                Some(path) => Some(path),
                None => store_favicon(client, url, fav_dir, &fav_path, dark, html.as_deref()).await,
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
/// then the conventional root icon, then the DuckDuckGo proxy.
async fn store_favicon(
    client: &reqwest::Client,
    url: &str,
    fav_dir: &Path,
    fav_path: &Path,
    dark: bool,
    html: Option<&str>,
) -> Option<PathBuf> {
    let mut fetched = false;
    match html {
        Some(html) => {
            if dark && fetch_dark_favicon(client, url, fav_dir, fav_path, html).await {
                fetched = true;
            }
            if !fetched && fetch_site_favicon(client, url, fav_dir, fav_path, html).await {
                fetched = true;
            }
        }
        // No page, so no declarations to read. The conventional root icon costs
        // one request and is the same path a browser falls back to; sites behind
        // a bot filter that 404s their HTML (crates.io) still serve it.
        None => {
            if let Some(root) = root_favicon_url(url)
                && fetch_icon(client, &root, fav_dir, fav_path).await
            {
                fetched = true;
            }
        }
    }
    if !fetched {
        let domain = extract_domain(url)?;
        let fallback_url = format!("https://icons.duckduckgo.com/ip3/{domain}.ico");
        fetched = fetch_icon(client, &fallback_url, fav_dir, fav_path).await;
    }
    if !fetched {
        return None;
    }
    Some(fav_path.to_path_buf())
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
/// `-dark` siblings, saving the first candidate that decodes as an icon. A
/// soft-404 that answers 200 with HTML must not stop the probe chain.
async fn fetch_dark_favicon(
    client: &reqwest::Client,
    base_url: &str,
    fav_dir: &Path,
    fav_path: &Path,
    html: &str,
) -> bool {
    let (direct, probes) = find_dark_favicon_candidates(html, base_url);
    for url in direct.into_iter().chain(probes) {
        if fetch_icon(client, &url, fav_dir, fav_path).await {
            return true;
        }
    }
    false
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
    fav_dir: &Path,
    fav_path: &Path,
    html: &str,
) -> bool {
    // The spec's default icon is the last `rel="icon"` without a `sizes`
    // attribute; try from the end so an overriding declaration wins, then work
    // backwards through the rest.
    for url in site_favicon_hrefs(html, base_url).into_iter().rev() {
        if fetch_icon(client, &url, fav_dir, fav_path).await {
            return true;
        }
    }
    false
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

/// The root icon a browser requests when a page declares none. Used when the
/// page could not be read, so its own declarations are unknown.
fn root_favicon_url(url: &str) -> Option<String> {
    url::Url::parse(url)
        .ok()?
        .join("/favicon.ico")
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

/// Title cache file for one page, keyed by [`url_cache_key`].
fn title_cache_path(url: &str, fav_dir: &Path) -> Option<PathBuf> {
    Some(fav_dir.join(format!("{}.title", url_cache_key(url)?)))
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

/// Pages (and theme variants) for which a favicon fetch has already been
/// dispatched this session. Keyed by `url-key:dark` so switching theme still
/// fetches the other variant. Inserted at dispatch time so
/// `check_pending_favicons` neither double-fetches an in-flight page across
/// overlapping refreshes nor re-fetches a page whose favicon cannot be
/// obtained (no site icon + DuckDuckGo 404) on every refresh. A successful
/// fetch reloads all matching rows, so no removal is needed. Per-session only:
/// cleared on restart, so a site that adds a favicon later is retried.
fn attempted_favicons() -> &'static Mutex<HashSet<String>> {
    static ATTEMPTED: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();
    ATTEMPTED.get_or_init(|| Mutex::new(HashSet::new()))
}

/// Forget every per-session dispatch, so "Clear caches" can re-fetch icons for
/// pages already attempted in this run. Without this the reset failure ledger
/// has no effect until a restart.
pub fn reset_attempted_favicons() {
    attempted_favicons()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .clear();
}

/// A clip stops being fetched after this many failed favicon attempts. The
/// ledger is persistent and only reset by the "Clear caches" maintenance
/// action, so a link with no obtainable favicon costs at most a handful of
/// requests ever instead of one per session forever.
///
/// The ledger itself lives in `cliptoo_core::favicon_failures` so the
/// maintenance pass can prune counts for clips it deletes.
///
/// After populating the clip list, scan for link clips without their own
/// cached favicon and fetch them in the background. The model is reloaded as
/// icons land, so a long batch fills in progressively.
///
/// Must be called on the UI thread (e.g. inside `upgrade_in_event_loop`)
/// because it accesses the Slint model synchronously to collect pending
/// clip IDs.  The actual HTTP fetching happens on the tokio runtime.
pub fn check_pending_favicons(ui: &crate::AppWindow, db: &Arc<DbPool>, favicons_dir: &Path) {
    let model = ui.get_clips();
    let mut pending = Vec::new();
    let mut attempted = attempted_favicons()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    // The active theme decides which cache file is fetched, so the ledger must
    // be keyed by variant too: a page attempted in dark mode must still be
    // fetched after switching to light (and vice versa).
    let dark = crate::theme::cached_resolved_theme().0;
    // Loaded on first use: a list whose links all have icons never pays for it.
    let mut failures: Option<HashMap<i64, u32>> = None;
    for i in 0..model.row_count() {
        if let Some(data) = model.row_data(i) {
            if data.clip_type.as_str() != "link" {
                continue;
            }
            let Some(key) = url_cache_key(&data.preview_content) else {
                continue;
            };
            // The page's own cache file decides; a page never renders another
            // page's icon.
            if favicons_dir.join(favicon_file_name(&key, dark)).exists() {
                continue;
            }
            // Budget exhausted: never fetch this clip again (until caches are
            // cleared) — the favicon has proven unobtainable.
            let failures = failures.get_or_insert_with(|| favicon_failures::load(favicons_dir));
            if favicon_failures::exhausted(failures, data.id as i64) {
                continue;
            }
            if attempted.insert(format!("{key}:{dark}")) {
                pending.push(data.id as i64);
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
    // one spawned task per link (the old code) opened hundreds of requests at
    // once, and every success reloaded the whole list, for O(links × rows) UI
    // work.
    let weak = ui.as_weak();
    let db = db.clone();
    let fav_dir = favicons_dir.to_owned();
    tokio::spawn(async move {
        let fetch_dir = fav_dir.clone();
        let mut stream = std::pin::pin!(
            futures::stream::iter(pending.into_iter().map(move |clip_id| {
                let db = db.clone();
                let fav_dir = fetch_dir.clone();
                async move {
                    let content = db
                        .with(|conn| cliptoo_core::db::queries::get_clip_content(conn, clip_id))
                        .await
                        .ok()?;
                    let key = url_cache_key(&content);
                    let fetched = fetch_favicon(&content, &fav_dir, dark).await.is_some();
                    Some((clip_id, key, fetched))
                }
            },))
            .buffer_unordered(FAVICON_FETCH_CONCURRENCY)
        );

        let mut outcomes = Vec::new();
        let mut reload_keys = HashSet::new();
        let mut completed = 0usize;
        while let Some(Some((clip_id, key, fetched))) = stream.next().await {
            outcomes.push((clip_id, fetched));
            if fetched && let Some(key) = key {
                reload_keys.insert(key);
            }
            completed += 1;
            if completed.is_multiple_of(FAVICON_RELOAD_EVERY) {
                flush_favicon_reload(&weak, &fav_dir, &mut reload_keys);
            }
        }
        flush_favicon_reload(&weak, &fav_dir, &mut reload_keys);
        // One read-modify-write for the whole batch; a per-clip update would be
        // O(clips²) file writes.
        favicon_failures::record(&fav_dir, &outcomes);
    });
}

/// Send the icons that just landed to the UI, so rows switch to their page's
/// icon as soon as it is fetched.
fn flush_favicon_reload(
    weak: &slint::Weak<crate::AppWindow>,
    fav_dir: &Path,
    keys: &mut HashSet<String>,
) {
    if keys.is_empty() {
        return;
    }
    let keys = std::mem::take(keys);
    let fav_dir = fav_dir.to_owned();
    let _ = weak.upgrade_in_event_loop(move |ui| {
        crate::thumbnail_cache::reload_favicons_for(&ui, &fav_dir, &keys);
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
    fn resolves_root_favicon_for_any_page_path() {
        assert_eq!(
            root_favicon_url("https://crates.io/crates/zbus").unwrap(),
            "https://crates.io/favicon.ico"
        );
        assert_eq!(
            root_favicon_url("http://localhost:8080/a/b?c=1#d").unwrap(),
            "http://localhost:8080/favicon.ico"
        );
        assert_eq!(root_favicon_url("not a url"), None);
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
