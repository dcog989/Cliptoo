use slint::ComponentHandle;
use slint::Image;
use slint::Model;
use std::collections::HashSet;
use std::num::NonZeroUsize;
use std::path::Path;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Mutex, OnceLock};

use cliptoo_core::db::models::ClipData as DbClipData;
use cliptoo_core::db::models::ClipType;
pub use cliptoo_core::image::hash_key;
use cliptoo_core::url::url_cache_key;

use lru::LruCache;

// ── In-memory image cache ─────────────────────────────────────────────────────

/// Default limit for both decoded-image caches: 512 entries. A list-cell WebP
/// is ~2–4 KiB on disk, but the cache holds decoded pixels, so the budget is an
/// entry count; Slint does not expose the buffer size.
const LRU_CAPACITY: usize = 512;

/// Least-Recently-Used cache of decoded Slint images, keyed by cache-file stem
/// (`hash[..16]`) or `url-key` / `url-key:dark` for favicons. `slint::Image` is
/// not `Send`, so the caches live on the UI thread.
pub struct ImageLru(LruCache<String, Image>);

impl ImageLru {
    pub fn new(capacity: usize) -> Self {
        Self(LruCache::new(
            NonZeroUsize::new(capacity).expect("capacity must be > 0"),
        ))
    }

    /// The cached image for `key`, otherwise the result of `load`. A miss is
    /// not cached: a thumbnail file can be written after its row was first
    /// converted, and caching "no image" would hide it until eviction.
    fn get_or_load(&mut self, key: &str, load: impl FnOnce() -> Option<Image>) -> Image {
        if let Some(img) = self.0.get(key) {
            return img.clone();
        }
        let Some(img) = load() else {
            return Image::default();
        };
        self.0.put(key.to_string(), img.clone());
        img
    }

    fn evict_where(&mut self, keep: impl Fn(&str) -> bool) {
        self.0.retain(|key, _| keep(key));
    }

    pub fn clear(&mut self) {
        self.0.clear();
    }
}

impl Default for ImageLru {
    fn default() -> Self {
        Self::new(LRU_CAPACITY)
    }
}

fn load_thumbnail(thumbnails_dir: &Path, hash: &str) -> Option<Image> {
    let key = hash_key(hash)?;
    let webp = thumbnails_dir.join(format!("{key}.webp"));
    if webp.exists() {
        return Image::load_from_path(&webp).ok();
    }
    let svg = thumbnails_dir.join(format!("{key}.svg"));
    // A 0-byte SVG (a copied empty source file from before validation was
    // added) would make Slint log a parse error; treat it as absent.
    if svg.metadata().is_ok_and(|m| m.len() > 0) {
        return Image::load_from_path(&svg).ok();
    }
    None
}

fn thumbnail(thumbnails_dir: &Path, hash: &str) -> Image {
    let Some(key) = hash_key(hash) else {
        // Malformed hash: can never have keyed a real thumbnail file.
        return Image::default();
    };
    THUMB_LRU.with(|lru| {
        lru.borrow_mut()
            .get_or_load(key, || load_thumbnail(thumbnails_dir, hash))
    })
}

// ── Favicon cache ─────────────────────────────────────────────────────────────

/// Cached or on-disk favicon for a link clip. The key carries the theme
/// variant because the light and dark icons for a page are distinct files
/// and both may be cached side-by-side on disk.
fn favicon(favicons_dir: &Path, content: &str) -> Image {
    let Some(key) = url_cache_key(content) else {
        return Image::default();
    };
    let dark = crate::theme::cached_resolved_theme().0;
    let lru_key = if dark {
        format!("{key}:dark")
    } else {
        key.clone()
    };
    FAVICON_LRU.with(|lru| {
        lru.borrow_mut().get_or_load(&lru_key, || {
            let path = favicons_dir.join(crate::favicon::favicon_file_name(&key, dark));
            path.exists()
                .then(|| Image::load_from_path(&path).ok())
                .flatten()
        })
    })
}

/// Re-load every link clip's favicon from disk under the currently active
/// theme. Call on the UI thread after the theme changes so rows switch to
/// the matching light/dark favicon variant in place.
pub fn reload_favicons(ui: &crate::AppWindow, favicons_dir: &Path) {
    reload_favicons_where(ui, favicons_dir, &|_| true);
}

/// Re-load only the rows whose icon just got fetched, so a favicon fetch
/// batch does not re-read every link row's image on the UI thread.
pub fn reload_favicons_for(ui: &crate::AppWindow, favicons_dir: &Path, keys: &HashSet<String>) {
    reload_favicons_where(ui, favicons_dir, &|key| keys.contains(key));
}

fn reload_favicons_where(ui: &crate::AppWindow, favicons_dir: &Path, keep: &dyn Fn(&str) -> bool) {
    FAVICON_LRU.with(|lru| {
        lru.borrow_mut()
            .evict_where(|key| keep(key.strip_suffix(":dark").unwrap_or(key)))
    });
    let model = ui.get_clips();
    for i in 0..model.row_count() {
        if let Some(mut data) = model.row_data(i)
            && data.clip_type.as_str() == "link"
        {
            let key = url_cache_key(&data.preview_content);
            if !key.as_deref().is_some_and(keep) {
                continue;
            }
            data.favicon_image = favicon(favicons_dir, &data.preview_content);
            model.set_row_data(i, data);
        }
    }
}

/// Convert a DB clip to a Slint ClipData, loading the thumbnail from disk for
/// file_image clips and SVG source clips (via LRU cache) and the favicon for
/// link clips. Must be called on the UI thread because `slint::Image` is not Send.
pub fn convert(db_clip: DbClipData, thumbnails_dir: &Path, favicons_dir: &Path) -> crate::ClipData {
    // SVG source clips stay Text/CodeSnippet for preview and paste, but get a
    // rasterised list-cell thumbnail written at ingest time.
    let is_svg_source = matches!(db_clip.clip_type, ClipType::Text | ClipType::CodeSnippet)
        && cliptoo_core::content::is_svg_markup(&db_clip.preview_content);
    let thumbnail = if db_clip.clip_type == ClipType::FileImage || is_svg_source {
        thumbnail(thumbnails_dir, &db_clip.content_hash)
    } else {
        Image::default()
    };
    let favicon = if db_clip.clip_type == ClipType::Link {
        favicon(favicons_dir, &db_clip.preview_content)
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
    // The stored preview is capped at 200 bytes; a longer clip is not fully
    // readable from the row even when that preview fits it. Rich text previews
    // the stripped plain text, so the raw-markup byte size says nothing about
    // the cut — `build_preview` marks a truncated preview with an ellipsis.
    let preview_truncated = match db_clip.clip_type {
        ClipType::Rtf | ClipType::Html => db_clip.preview_content.ends_with('…'),
        _ => db_clip.size_in_bytes > db_clip.preview_content.len() as i64,
    };

    let match_context_str = db_clip.match_context.as_deref().unwrap_or("");
    let match_spans = if match_context_str.is_empty() {
        slint::ModelRc::default()
    } else {
        crate::search::parse_match_spans(match_context_str)
    };

    crate::ClipData {
        id: db_clip.id as i32,
        preview_content: db_clip.preview_content.into(),
        clip_type: db_clip.clip_type.as_str().into(),
        is_bookmarked: db_clip.is_bookmarked,
        has_leading_whitespace,
        is_multiline,
        preview_truncated,
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
    pub static THUMB_LRU: std::cell::RefCell<ImageLru> =
        std::cell::RefCell::new(ImageLru::default());
    pub static FAVICON_LRU: std::cell::RefCell<ImageLru> =
        std::cell::RefCell::new(ImageLru::default());
}

/// Evict every thumbnail and favicon image from the in-memory LRUs.
///
/// Must be called on the UI thread: the caches are thread-local and hold
/// non-Send `slint::Image` values. Used after cache pruning so the decoded
/// pixels are actually freed and the next list refresh re-reads from disk
/// instead of re-serving images whose files were just removed.
pub fn clear_caches() {
    THUMB_LRU.with(|lru| lru.borrow_mut().clear());
    FAVICON_LRU.with(|lru| lru.borrow_mut().clear());
    // Re-allow a generation attempt for every clip, so "Clear caches" rebuilds
    // the rows on screen even for clips that failed to regenerate before.
    attempted_thumbnails()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .clear();
}

/// Logical max dimension for a generated preview thumbnail, mirrored from the
/// `hover_image_preview_size` setting so list-driven generation can size the
/// preview without a settings handle.
static PREVIEW_MAX_DIM: AtomicU32 = AtomicU32::new(300);

pub fn set_preview_max_dim(dim: u32) {
    PREVIEW_MAX_DIM.store(dim, Ordering::Relaxed);
}

fn preview_max_dim() -> u32 {
    PREVIEW_MAX_DIM.load(Ordering::Relaxed)
}

/// Clip ids whose thumbnail generation was already attempted this session.
/// Without this, a clip whose source is gone would be decoded again on every
/// list refresh; `clear_caches` resets it.
fn attempted_thumbnails() -> &'static Mutex<HashSet<i64>> {
    static ATTEMPTED: OnceLock<Mutex<HashSet<i64>>> = OnceLock::new();
    ATTEMPTED.get_or_init(|| Mutex::new(HashSet::new()))
}

/// Generate the missing thumbnail for every displayed row that can have one:
/// image clips and SVG source clips. Called after the list model is built or
/// extended, so a clip regenerates only while it is on screen. Must run on the
/// UI thread (reads the Slint model); the decode and write happen in the
/// background.
pub fn check_pending_thumbnails(
    ui: &crate::AppWindow,
    db: &std::sync::Arc<cliptoo_core::db::DbPool>,
    thumbnails_dir: &Path,
    favicons_dir: &Path,
) {
    let model = ui.get_clips();
    let mut pending = Vec::new();
    let mut attempted = attempted_thumbnails()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    for i in 0..model.row_count() {
        let Some(data) = model.row_data(i) else {
            continue;
        };
        if data.thumbnail_image.size().width != 0 {
            continue;
        }
        let ct = data.clip_type.as_str();
        let can_have_thumbnail = ct == "file_image"
            || ((ct == "text" || ct == "code_snippet")
                && cliptoo_core::content::is_svg_markup(&data.preview_content));
        if can_have_thumbnail && attempted.insert(data.id as i64) {
            pending.push(data.id as i64);
        }
    }
    drop(attempted);
    if pending.is_empty() {
        return;
    }

    let max_dim = preview_max_dim();
    let db = db.clone();
    let td = thumbnails_dir.to_path_buf();
    let fd = favicons_dir.to_path_buf();
    let weak = ui.as_weak();
    tokio::spawn(async move {
        let mut written = false;
        for id in pending {
            let Ok((content, clip_type, hash)) = db
                .with(|conn| cliptoo_core::db::queries::get_clip_type_and_content(conn, id))
                .await
            else {
                continue;
            };
            let td2 = td.clone();
            // One decode at a time, like ingest: a hover or another ingest
            // must not overlap this.
            let _permit = crate::decode_limit::permit().await;
            let stored = tokio::task::spawn_blocking(move || -> anyhow::Result<()> {
                if clip_type == "file_image" {
                    // A multi-image selection stores one path per line; the
                    // thumbnail describes the first file, as at ingest.
                    let path = content.lines().next().unwrap_or(&content);
                    cliptoo_core::image::store_both_thumbnails_for_file(
                        &td2,
                        &hash,
                        Path::new(path),
                        max_dim,
                    )
                } else if cliptoo_core::content::is_svg_markup(&content) {
                    cliptoo_core::image::store_both_thumbnails(
                        &td2,
                        &hash,
                        &cliptoo_core::image::svg_thumbnail_bytes(&content),
                        max_dim,
                    )
                } else {
                    Ok(())
                }
            })
            .await;
            match stored {
                Ok(Ok(())) => written = true,
                Ok(Err(e)) => tracing::debug!("thumbnail rebuild for clip {id}: {e:#}"),
                Err(e) => tracing::warn!("thumbnail rebuild for clip {id} task failed: {e}"),
            }
        }
        if written {
            let _ = weak.upgrade_in_event_loop(move |ui| {
                crate::helpers::refresh_clips_live(&db, &ui.as_weak(), &td, &fd, None);
            });
        }
    });
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
