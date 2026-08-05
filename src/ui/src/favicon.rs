use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::helpers::extract_domain;
use cliptoo_core::db::DbPool;
use slint::ComponentHandle;
use slint::Model;

const FAVICON_FETCH_TIMEOUT_SECS: u64 = 3;

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
    if dark {
        let base_url = format!("https://{domain}");
        if let Some(bytes) = fetch_dark_favicon(&client, &base_url).await
            && save_favicon(&fav_path, &bytes)
        {
            return Some(fav_path);
        }
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
    let tag_re = regex::Regex::new(LINK_TAG_RE).ok();
    let attr_re = regex::Regex::new(LINK_ATTR_RE).ok();
    let (Some(tag_re), Some(attr_re)) = (tag_re, attr_re) else {
        return (Vec::new(), Vec::new());
    };
    let mut direct = Vec::new();
    let mut probes = Vec::new();
    for cap in tag_re.captures_iter(html) {
        let Some(tag) = cap.get(0) else {
            continue;
        };
        let mut rel: Option<String> = None;
        let mut media: Option<String> = None;
        let mut href: Option<String> = None;
        let mut base: Option<String> = None;
        for a in attr_re.captures_iter(tag.as_str()) {
            match &a[1] {
                "rel" => rel = Some(a[2].to_ascii_lowercase()),
                "media" => media = Some(a[2].to_ascii_lowercase()),
                "href" => href = Some(a[2].to_string()),
                "data-base-href" => base = Some(a[2].to_string()),
                _ => {}
            }
        }
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

/// Resolve a (possibly relative) favicon `href` against the page's base URL.
fn resolve_url(base_url: &str, href: &str) -> Option<String> {
    if href.starts_with("http://") || href.starts_with("https://") {
        return Some(href.to_string());
    }
    if let Some(rest) = href.strip_prefix("//") {
        return Some(format!("https://{rest}"));
    }
    if let Some(path) = href.strip_prefix('/') {
        return Some(format!("{base_url}/{path}"));
    }
    Some(format!("{base_url}/{href}"))
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

pub fn load_cached_page_title(url: &str, fav_dir: &Path) -> Option<String> {
    let domain = extract_domain(url)?;
    let path = fav_dir.join(format!("{domain}.title"));
    std::fs::read_to_string(&path).ok()
}

pub fn cache_page_title(url: &str, title: &str, fav_dir: &Path) {
    let domain = match extract_domain(url) {
        Some(d) => d,
        None => return,
    };
    let path = fav_dir.join(format!("{domain}.title"));
    let _ = std::fs::write(&path, title);
}

/// After populating the clip list, scan for link clips without cached
/// favicons and fetch them in the background.  Updates the model row
/// in-place as each favicon arrives.
///
/// Must be called on the UI thread (e.g. inside `upgrade_in_event_loop`)
/// because it accesses the Slint model synchronously to collect pending
/// clip IDs.  The actual HTTP fetching happens on the tokio runtime.
pub fn check_pending_favicons(ui: &crate::AppWindow, db: &Arc<DbPool>, favicons_dir: &Path) {
    let model = ui.get_clips();
    let mut pending = Vec::new();
    for i in 0..model.row_count() {
        if let Some(data) = model.row_data(i) {
            let ct = data.clip_type.as_str();
            if ct == "link" && data.favicon_image.size().width == 0 {
                pending.push((i, data.id as i64));
            }
        }
    }
    if pending.is_empty() {
        return;
    }
    // Fetch the variant matching the current theme (dark cache entries are
    // fetched separately from light ones).
    let dark = crate::theme::cached_resolved_theme().0;
    let weak = ui.as_weak();
    let db = db.clone();
    let fav_dir = favicons_dir.to_owned();
    for (row, clip_id) in pending {
        let weak = weak.clone();
        let db = db.clone();
        let fav_dir = fav_dir.clone();
        tokio::spawn(async move {
            if let Ok(content) = db
                .with(|conn| cliptoo_core::db::queries::get_clip_content(conn, clip_id))
                .await
                && let Some(fav_path) = fetch_favicon(&content, &fav_dir, dark).await
            {
                let _ = weak.upgrade_in_event_loop(move |ui| {
                    let img = slint::Image::load_from_path(&fav_path).unwrap_or_default();
                    if img.size().width == 0 {
                        return;
                    }
                    let model = ui.get_clips();
                    if let Some(mut data) = model.row_data(row) {
                        data.favicon_image = img;
                        model.set_row_data(row, data);
                    }
                });
            }
        });
    }
}
