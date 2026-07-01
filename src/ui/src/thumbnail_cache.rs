use slint::Image;
use std::num::NonZeroUsize;
use std::path::Path;

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
        let path = thumbnails_dir.join(format!("{key}.webp"));
        let img = if path.exists() {
            Image::load_from_path(&path).unwrap_or_default()
        } else {
            Image::default()
        };
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
    let path = thumbnails_dir.join(format!(
        "{}.webp",
        &content_hash[..content_hash.len().min(HASH_FILENAME_PREFIX_LEN)]
    ));
    if path.exists() {
        Image::load_from_path(&path).unwrap_or_default()
    } else {
        Image::default()
    }
}

/// Extract the domain from a URL (e.g. "https://github.com/foo" -> "github.com").
fn extract_domain(url: &str) -> Option<String> {
    let stripped = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))?;
    Some(stripped.split('/').next()?.to_string())
}

fn load_favicon(favicons_dir: &Path, content: &str) -> Image {
    let domain = match extract_domain(content) {
        Some(d) => d,
        None => return Image::default(),
    };
    let path = favicons_dir.join(format!("{domain}.webp"));
    if path.exists() {
        Image::load_from_path(&path).unwrap_or_default()
    } else {
        Image::default()
    }
}

/// Parse an FTS5 match-context string with `[HL]...[/HL]` sentinels
/// into a sequence of `MatchSpan` structs for inline highlighting.
///
/// E.g. `"foo [HL]bar[/HL] baz"` →
///   `[("foo ", false), ("bar", true), (" baz", false)]`
fn parse_match_spans(context: &str) -> slint::ModelRc<crate::MatchSpan> {
    use cliptoo_core::db::queries::{FTS_HL_CLOSE, FTS_HL_OPEN};

    let mut spans: Vec<crate::MatchSpan> = Vec::new();
    let mut rest = context;
    while !rest.is_empty() {
        if let Some(hl_start) = rest.find(FTS_HL_OPEN) {
            if hl_start > 0 {
                spans.push(crate::MatchSpan {
                    text: rest[..hl_start].into(),
                    is_highlight: false,
                });
            }
            let after_open = &rest[hl_start + FTS_HL_OPEN.len()..];
            if let Some(hl_end) = after_open.find(FTS_HL_CLOSE) {
                spans.push(crate::MatchSpan {
                    text: after_open[..hl_end].into(),
                    is_highlight: true,
                });
                rest = &after_open[hl_end + FTS_HL_CLOSE.len()..];
            } else {
                // Unclosed [HL] — treat remainder as plain
                spans.push(crate::MatchSpan {
                    text: after_open.into(),
                    is_highlight: false,
                });
                break;
            }
        } else {
            spans.push(crate::MatchSpan {
                text: rest.into(),
                is_highlight: false,
            });
            break;
        }
    }
    slint::ModelRc::from(std::rc::Rc::new(slint::VecModel::from(spans)))
}

/// Convert a DB clip to a Slint ClipData, loading the thumbnail
/// from disk for file_image clips (via LRU cache) and the favicon for link clips.
/// Must be called on the UI thread because `slint::Image` is not Send.
pub fn convert(
    db_clip: DbClipData,
    thumbnails_dir: &Path,
    favicons_dir: &Path,
    lru: Option<&mut ThumbnailLru>,
) -> crate::ClipData {
    let thumbnail = if db_clip.clip_type == ClipType::FileImage {
        if let Some(cache) = lru {
            cache.get_or_load(thumbnails_dir, &db_clip.content_hash)
        } else {
            load_thumbnail(thumbnails_dir, &db_clip.content_hash)
        }
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
        parse_match_spans(match_context_str)
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

/// Convert a Vec of DB clips to Slint ClipData, using the LRU cache for thumbnails.
pub fn convert_vec(
    clips: Vec<DbClipData>,
    thumbnails_dir: &Path,
    favicons_dir: &Path,
) -> Vec<crate::ClipData> {
    clips
        .into_iter()
        .map(|d| convert(d, thumbnails_dir, favicons_dir, None))
        .collect()
}

// Thread-local LRU thumbnail cache — allocated once on the UI thread.
thread_local! {
    pub static THUMB_LRU: std::cell::RefCell<ThumbnailLru> =
        std::cell::RefCell::new(ThumbnailLru::default());
}

/// Convert a Vec of DB clips using the provided LRU cache.
pub fn convert_vec_cached(
    clips: Vec<DbClipData>,
    thumbnails_dir: &Path,
    favicons_dir: &Path,
    lru: &mut ThumbnailLru,
) -> Vec<crate::ClipData> {
    clips
        .into_iter()
        .map(|d| convert(d, thumbnails_dir, favicons_dir, Some(lru)))
        .collect()
}
