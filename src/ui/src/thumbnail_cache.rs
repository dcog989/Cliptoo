use slint::Image;
use slint::Model;
use std::num::NonZeroUsize;
use std::path::Path;

use crate::helpers::extract_domain;
use cliptoo_core::db::models::ClipData as DbClipData;
use cliptoo_core::db::models::ClipType;
use cliptoo_core::image::HASH_FILENAME_PREFIX_LEN;

use lru::LruCache;

// ── LRU thumbnail cache ───────────────────────────────────────────────────────

/// Default in-memory thumbnail cache limit: 32 MiB worth of decoded pixels.
/// Each list-cell WebP is typically ~2–4 KiB on disk; once decoded into RGBA
/// the Slint Image carries the pixel buffer internally.  The budget here is
/// expressed as the number of cached entries (not pixel bytes) since Slint
/// does not expose the buffer size.  At ~50 KB average per thumbnail image
/// 512 entries ≈ 25 MB.
const LRU_CAPACITY: usize = 512;

/// Least-Recently-Used in-memory cache for decoded Slint thumbnail images.
///
/// Wraps `lru::LruCache<String, slint::Image>`.  Keyed by the first 16
/// characters of the content hash (the same prefix used for the WebP filename
/// on disk).  `slint::Image` is not `Send` so the cache must live on the
/// UI thread.
pub struct ThumbnailLru(LruCache<String, Image>);

impl ThumbnailLru {
    pub fn new(capacity: usize) -> Self {
        Self(LruCache::new(
            NonZeroUsize::new(capacity).expect("capacity must be > 0"),
        ))
    }

    pub fn get_or_load(&mut self, thumbnails_dir: &Path, hash: &str) -> Image {
        let key = &hash[..hash.len().min(HASH_FILENAME_PREFIX_LEN)];
        if let Some(img) = self.0.get(key) {
            return img.clone();
        }
        let img = load_thumbnail(thumbnails_dir, hash);
        self.0.put(key.to_string(), img.clone());
        img
    }
}

impl Default for ThumbnailLru {
    fn default() -> Self {
        Self::new(LRU_CAPACITY)
    }
}

fn load_thumbnail(thumbnails_dir: &Path, content_hash: &str) -> Image {
    let key = &content_hash[..content_hash.len().min(HASH_FILENAME_PREFIX_LEN)];
    let webp = thumbnails_dir.join(format!("{key}.webp"));
    if webp.exists() {
        return Image::load_from_path(&webp).unwrap_or_default();
    }
    let svg = thumbnails_dir.join(format!("{key}.svg"));
    if svg.exists() {
        return Image::load_from_path(&svg).unwrap_or_default();
    }
    Image::default()
}

// ── LRU favicon cache ─────────────────────────────────────────────────────────

/// Least-Recently-Used in-memory cache for decoded Slint favicon images.
/// Keyed by domain plus the theme variant, since the light and dark icons for
/// a domain are distinct and both may be cached side-by-side on disk.
/// `slint::Image` is not `Send` so the cache must live on the UI thread.
pub struct FaviconLru(LruCache<String, Image>);

impl FaviconLru {
    pub fn get_or_load(&mut self, favicons_dir: &Path, content: &str) -> Image {
        let domain = match extract_domain(content) {
            Some(d) => d,
            None => return Image::default(),
        };
        let dark = crate::theme::cached_resolved_theme().0;
        let cache_key = if dark {
            format!("{domain}:dark")
        } else {
            domain.clone()
        };
        if let Some(img) = self.0.get(&cache_key) {
            return img.clone();
        }
        let path = favicons_dir.join(crate::favicon::favicon_file_name(&domain, dark));
        let img = if path.exists() {
            Image::load_from_path(&path).unwrap_or_default()
        } else {
            Image::default()
        };
        self.0.put(cache_key, img.clone());
        img
    }

    pub fn clear(&mut self) {
        self.0.clear();
    }
}

impl Default for FaviconLru {
    fn default() -> Self {
        Self(LruCache::new(
            NonZeroUsize::new(LRU_CAPACITY).expect("capacity must be > 0"),
        ))
    }
}

fn load_favicon(favicons_dir: &Path, content: &str) -> Image {
    FAVICON_LRU.with(|lru| lru.borrow_mut().get_or_load(favicons_dir, content))
}

/// Re-load every link clip's favicon from disk under the currently active
/// theme. Call on the UI thread after the theme changes so rows switch to
/// the matching light/dark favicon variant in place.
pub fn reload_favicons(ui: &crate::AppWindow, favicons_dir: &Path) {
    FAVICON_LRU.with(|lru| lru.borrow_mut().clear());
    let model = ui.get_clips();
    for i in 0..model.row_count() {
        if let Some(mut data) = model.row_data(i)
            && data.clip_type.as_str() == "link"
        {
            data.favicon_image = load_favicon(favicons_dir, &data.preview_content);
            model.set_row_data(i, data);
        }
    }
}

/// Convert a DB clip to a Slint ClipData, loading the thumbnail
/// from disk for file_image clips (via LRU cache) and the favicon for link clips.
/// Must be called on the UI thread because `slint::Image` is not Send.
pub fn convert(db_clip: DbClipData, thumbnails_dir: &Path, favicons_dir: &Path) -> crate::ClipData {
    let thumbnail = if db_clip.clip_type == ClipType::FileImage {
        THUMB_LRU.with(|lru| {
            lru.borrow_mut()
                .get_or_load(thumbnails_dir, &db_clip.content_hash)
        })
    } else {
        Image::default()
    };
    let favicon = if db_clip.clip_type == ClipType::Link {
        load_favicon(favicons_dir, &db_clip.preview_content)
    } else {
        Image::default()
    };
    // Parse the clip colour for Color-type clips so the list-row swatch shows
    // the actual colour rather than a placeholder grey.
    let clip_color = if db_clip.clip_type == ClipType::Color {
        cliptoo_core::color::ColorParser::try_parse(&db_clip.preview_content)
            .map(|c| slint::Color::from_argb_u8(c.a, c.r, c.g, c.b))
            .unwrap_or(slint::Color::from_argb_u8(0, 0, 0, 0))
    } else {
        slint::Color::from_argb_u8(0, 0, 0, 0)
    };
    let has_leading_whitespace = db_clip.has_leading_whitespace;
    let is_multiline = db_clip.is_multiline;

    let match_context_str = db_clip.match_context.as_deref().unwrap_or("");
    let match_spans = if match_context_str.is_empty() {
        slint::ModelRc::default()
    } else {
        crate::search::parse_match_spans(match_context_str)
    };

    crate::ClipData {
        id: db_clip.id as i32,
        preview_content: db_clip.preview_content.into(),
        content_hash: db_clip.content_hash.into(),
        clip_type: db_clip.clip_type.as_str().into(),
        source_app: db_clip.source_app.unwrap_or_default().into(),
        timestamp: db_clip.timestamp.into(),
        is_bookmarked: db_clip.is_bookmarked,
        was_trimmed: db_clip.was_trimmed,
        has_leading_whitespace,
        is_multiline,
        size_in_bytes: db_clip.size_in_bytes as i32,
        paste_count: db_clip.paste_count as i32,
        tags: db_clip.tags.unwrap_or_default().into(),
        match_context: match_context_str.into(),
        match_spans,
        is_deadhead: db_clip.is_deadhead,
        thumbnail_image: thumbnail,
        favicon_image: favicon,
        clip_color,
    }
}

// Thread-local LRU caches — allocated once on the UI thread.
thread_local! {
    pub static THUMB_LRU: std::cell::RefCell<ThumbnailLru> =
        std::cell::RefCell::new(ThumbnailLru::default());
    pub static FAVICON_LRU: std::cell::RefCell<FaviconLru> =
        std::cell::RefCell::new(FaviconLru::default());
}

/// Convert a Vec of DB clips to Slint ClipData, using the thread-local
/// LRU caches for thumbnails and favicons.
pub fn convert_vec(
    clips: Vec<DbClipData>,
    thumbnails_dir: &Path,
    favicons_dir: &Path,
) -> Vec<crate::ClipData> {
    clips
        .into_iter()
        .map(|d| convert(d, thumbnails_dir, favicons_dir))
        .collect()
}
