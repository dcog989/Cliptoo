use std::cell::RefCell;
use std::io::Read;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, OnceLock};

use slint::{ComponentHandle, Model};

/// Horizontal offset (logical px) of the preview popup from the pointer.
const POPUP_OFFSET_X: f32 = 20.0;
/// Maximum bytes read from a copied file for its text preview, so a huge file
/// (e.g. a multi-GB log) is never slurped into memory just to render a tooltip.
const FILE_TEXT_PREVIEW_MAX_BYTES: usize = 64 * 1024;
/// Glyph repeated to separate a text file's path line from its contents in the
/// preview popup.
const FILE_PREVIEW_SEPARATOR_GLYPH: &str = "─";
/// Length (in glyphs) of the path/contents divider line.
const FILE_PREVIEW_SEPARATOR_LEN: usize = 40;
/// Entries visited for one folder hover. A huge or network-mounted folder must
/// not pin a blocking thread; the count is shown with a `+` past this limit.
const FOLDER_SCAN_MAX_ENTRIES: u64 = 5000;
/// Page and icon fetches allowed at once from hovering. A pointer sweep can ask
/// for many previews; stale requests are dropped before they take a permit.
const HOVER_FETCH_CONCURRENCY: usize = 4;

/// Bumped on every preview request and on every dismissal. A request still in
/// flight when the pointer leaves (or the window hides) must not show the
/// tooltip afterwards — nobody would be left to dismiss it.
static PREVIEW_GENERATION: AtomicU64 = AtomicU64::new(0);

/// Caps concurrent hover fetches so a pointer sweep cannot queue one page
/// download per row.
fn hover_fetch_limit() -> &'static tokio::sync::Semaphore {
    static LIMIT: OnceLock<tokio::sync::Semaphore> = OnceLock::new();
    LIMIT.get_or_init(|| tokio::sync::Semaphore::new(HOVER_FETCH_CONCURRENCY))
}

thread_local! {
    // The live preview window, registered at startup so modules without a
    // handle (e.g. `window::hide_window`) can hide it. UI-thread only.
    static PREVIEW_WINDOW: RefCell<Option<slint::Weak<crate::PreviewWindow>>> =
        const { RefCell::new(None) };
}

/// Remember the created preview window so [`hide_preview`] can reach it.
pub fn register_preview_window(window: &crate::PreviewWindow) {
    PREVIEW_WINDOW.with(|slot| *slot.borrow_mut() = Some(window.as_weak()));
}

/// Release the tooltip's payload and park it off-screen instead of hiding it.
/// Hiding a Wayland surface and showing it again allocates a fresh shm buffer
/// per hover; keeping it mapped reuses one.
fn park_preview(window: &crate::PreviewWindow) {
    window.set_preview_text("".into());
    window.set_preview_image(slint::Image::default());
    window.set_preview_favicon(slint::Image::default());
    window.set_preview_web_title("".into());
    window.set_preview_file_info("".into());
    window
        .window()
        .set_position(slint::LogicalPosition::new(-10000.0, -10000.0));
}

/// Hide the hover preview without a window handle. Called when the main window
/// hides, so a stale preview can't reappear orphaned on the next show.
pub fn hide_preview() {
    // Invalidate any request still in flight so a late `show()` cannot orphan
    // the tooltip after the main window hid or a dismissal ran.
    PREVIEW_GENERATION.fetch_add(1, Ordering::Relaxed);
    PREVIEW_WINDOW.with(|slot| {
        if let Some(window) = slot.borrow().as_ref().and_then(|w| w.upgrade()) {
            park_preview(&window);
        }
    });
}

/// Run `apply` on the preview window's own `Theme` global. Slint globals are
/// per-window instance; UI-thread only.
pub fn apply_theme(apply: &dyn Fn(&crate::Theme)) {
    PREVIEW_WINDOW.with(|slot| {
        if let Some(window) = slot.borrow().as_ref().and_then(|w| w.upgrade()) {
            apply(&window.global::<crate::Theme>());
        }
    });
}

/// Everything a preview handler needs, bundled so the per-type handlers can
/// share one uniform `fn(&PreviewContext)` signature and be dispatched from a
/// table instead of a growing `match`.
struct PreviewContext<'a> {
    /// The main window, for the clip model (link favicon row refresh).
    app: &'a crate::AppWindow,
    /// The standalone window that renders the popup.
    preview: &'a crate::PreviewWindow,
    clip_type: &'a str,
    content: &'a str,
    content_hash: &'a str,
    clip_id: i32,
    fav_dir: &'a Path,
    td: &'a Path,
    /// Max pixel dimension for an image preview generated on demand (the
    /// `hover_image_preview_size` setting), so on-demand generation matches
    /// the size used when the clip was ingested.
    preview_max_dim: u32,
    /// Generation of the request that built this context, plus the shared
    /// counter. Bumped on every request and on every dismissal; async
    /// completions (title/favicon, thumbnail generation, file reads) apply
    /// their result only while the counter still equals `request_generation`,
    /// so a slow result for a previously hovered clip can never overwrite a
    /// newer preview or reopen a dismissed one.
    request_generation: u64,
    generation: &'static AtomicU64,
}

/// Place the preview next to the pointer. Coordinates are main-window-local;
/// the preview is a Qt tooltip child of the main window, so Slint translates
/// them to screen coordinates. Clamped to the screen afterwards so a window
/// parked at an edge cannot open the preview mostly off-screen.
fn position_preview(app: &crate::AppWindow, preview: &crate::PreviewWindow, x: f32, y: f32) {
    preview
        .window()
        .set_position(slint::LogicalPosition::new(x + POPUP_OFFSET_X, y));
    crate::drag::clamp_tooltip(preview, app);
}

/// Preview for a code-snippet clip: the snippet text in a fixed-width popup.
fn show_code_preview(ctx: &PreviewContext) {
    ctx.preview.set_preview_clip_type("code_snippet".into());
    ctx.preview.set_preview_text(ctx.content.into());
}

/// Preview for a link clip: the URL plus the cached or fetched page title and
/// favicon, updating the row in-place when the favicon arrives.
fn show_link_preview(ctx: &PreviewContext) {
    let preview = ctx.preview;
    preview.set_preview_clip_type("link".into());
    preview.set_preview_text(ctx.content.into());
    preview.set_preview_favicon(slint::Image::default());
    preview.set_preview_web_title("".into());
    let c = ctx.content.to_string();
    let fd = ctx.fav_dir.to_path_buf();
    let clip_id = ctx.clip_id;
    let dark = crate::theme::cached_resolved_theme().0;
    let preview_w = preview.as_weak();
    let app_w = ctx.app.as_weak();
    // Read the cached title once and reuse it both for the immediate title and
    // the background branch decision below.
    let cached_title = crate::favicon::load_cached_page_title(&c, &fd);
    if let Some(ref t) = cached_title {
        preview.set_preview_web_title(t.clone().into());
    }
    let generation = ctx.request_generation;
    let generation_cell = ctx.generation;
    let has_cached_title = cached_title.is_some();
    tokio::spawn(async move {
        // A newer hover (or a dismissal) already replaced this request: never
        // start its network work.
        if generation_cell.load(Ordering::Relaxed) != generation {
            return;
        }
        let Ok(_permit) = hover_fetch_limit().acquire().await else {
            return;
        };
        if generation_cell.load(Ordering::Relaxed) != generation {
            return;
        }
        let (title, fav_path) = if has_cached_title {
            (None, crate::favicon::fetch_favicon(&c, &fd, dark).await)
        } else {
            // One page download yields both the title and the icon.
            let (t, f) = crate::favicon::fetch_link_meta(&c, &fd, dark).await;
            if let Some(ref t) = t {
                crate::favicon::cache_page_title(&c, t, &fd);
            }
            (t, f)
        };
        let _ = preview_w.upgrade_in_event_loop(move |preview| {
            // Ignore the result if a newer request has taken over the popup.
            if generation_cell.load(Ordering::Relaxed) != generation {
                return;
            }
            if let Some(t) = title.or(cached_title) {
                preview.set_preview_web_title(t.into());
            }
            if let Some(p) = fav_path {
                let img = slint::Image::load_from_path(&p).unwrap_or_default();
                preview.set_preview_favicon(img.clone());
                // Refresh the list row's favicon on the main window.
                if let Some(app) = app_w.upgrade() {
                    let model = app.get_clips();
                    for i in 0..model.row_count() {
                        if let Some(mut data) = model.row_data(i)
                            && data.id == clip_id
                        {
                            data.favicon_image = img;
                            model.set_row_data(i, data);
                            break;
                        }
                    }
                }
            }
        });
    });
}

/// Format a modification time in the user's local timezone, matching how the
/// folder info line renders its newest entry.
fn format_mtime(time: std::time::SystemTime) -> String {
    let dt: chrono::DateTime<chrono::Utc> = time.into();
    dt.with_timezone(&chrono::Local)
        .format("%Y-%m-%d %H:%M")
        .to_string()
}

/// Size and last-modified line for a single copied file, e.g.
/// `1.2 MB · 2026-10-31 18:36`. Empty when the path is not a readable file
/// (deleted, a directory, or a multi-selection payload).
fn file_info(path: &str) -> String {
    let Ok(meta) = std::fs::metadata(path) else {
        return String::new();
    };
    if !meta.is_file() {
        return String::new();
    }
    let size = crate::stats_ui::format_size(meta.len());
    match meta.modified() {
        Ok(mtime) => format!("{size} · {}", format_mtime(mtime)),
        Err(_) => size,
    }
}

/// Fill the preview's info line off the UI thread: a stat on a slow or network
/// path must not block the event loop.
fn spawn_file_info(ctx: &PreviewContext, path: String) {
    let ui = ctx.preview.as_weak();
    let generation = ctx.request_generation;
    let generation_cell = ctx.generation;
    tokio::spawn(async move {
        let info = tokio::task::spawn_blocking(move || file_info(&path)).await;
        let Ok(info) = info else {
            return;
        };
        let _ = ui.upgrade_in_event_loop(move |preview| {
            if generation_cell.load(Ordering::Relaxed) != generation {
                return;
            }
            preview.set_preview_file_info(info.into());
        });
    });
}

/// Preview for a file-image clip: the stored preview WebP/SVG if present,
/// otherwise generate it in the background and load the result when ready.
fn show_image_preview(ctx: &PreviewContext) {
    let preview = ctx.preview;
    let content = ctx.content;
    let content_hash = ctx.content_hash;
    let td = ctx.td;
    // A multi-selection image clip stores one path per line; the thumbnail and
    // file info describe the first file.
    let first_path = content.lines().next().unwrap_or(content).to_string();
    // A legacy/corrupt row can carry a hash too short (or not on a char
    // boundary) for the filename prefix; do not slice it blindly.
    let Some(prefix) = crate::thumbnail_cache::hash_key(content_hash) else {
        preview.set_preview_clip_type("file_image".into());
        preview.set_preview_text(content.into());
        preview.set_preview_file_info("".into());
        spawn_file_info(ctx, first_path);
        return;
    };
    let prefix = prefix.to_string();
    let preview_webp = td.join(format!("{prefix}_preview.webp"));
    let preview_svg = td.join(format!("{prefix}_preview.svg"));
    if preview_webp.exists() {
        let img = slint::Image::load_from_path(&preview_webp).unwrap_or_default();
        preview.set_preview_image(img);
    } else if preview_svg.metadata().is_ok_and(|m| m.len() > 0) {
        let img = slint::Image::load_from_path(&preview_svg).unwrap_or_default();
        preview.set_preview_image(img);
    } else {
        let file_path = first_path.clone();
        let store_td = td.to_path_buf();
        let td2 = td.to_path_buf();
        let hash2 = content_hash.to_string();
        let max_dim = ctx.preview_max_dim;
        let generation = ctx.request_generation;
        let generation_cell = ctx.generation;
        let w = preview.as_weak();
        tokio::spawn(async move {
            // Decoding, resizing and WebP encoding are blocking; keep them off
            // the async workers that also run the clipboard poll.
            let _ = tokio::task::spawn_blocking(move || {
                cliptoo_core::image::store_both_thumbnails_for_file(
                    &store_td,
                    &hash2,
                    Path::new(&file_path),
                    max_dim,
                )
            })
            .await;
            // The file-backed path writes a WebP, except for SVG sources which
            // are copied verbatim; pick whichever exists.
            let webp = td2.join(format!("{prefix}_preview.webp"));
            let svg = td2.join(format!("{prefix}_preview.svg"));
            let preview_path = if webp.exists() {
                Some(webp)
            } else {
                svg.metadata().is_ok_and(|m| m.len() > 0).then_some(svg)
            };
            let Some(preview_path) = preview_path else {
                return;
            };
            let _ = w.upgrade_in_event_loop(move |preview| {
                // A newer request has taken over the popup; leave it alone.
                if generation_cell.load(Ordering::Relaxed) != generation {
                    return;
                }
                let img = slint::Image::load_from_path(&preview_path).unwrap_or_default();
                preview.set_preview_image(img);
            });
        });
    }
    preview.set_preview_clip_type("file_image".into());
    preview.set_preview_text(content.into());
    // The caption is the stored path; add the size/date line below it.
    ctx.preview.set_preview_file_info("".into());
    spawn_file_info(ctx, first_path);
}

/// Preview for a folder clip: the path plus an entry count, total size and the
/// latest file modification date. The directory scan runs on a blocking thread
/// so a large folder (or slow network mount) can't stall the UI thread; the
/// path line is shown first so the popup is immediately non-empty.
fn show_folder_preview(ctx: &PreviewContext) {
    ctx.preview.set_preview_clip_type("folder".into());
    ctx.preview.set_preview_text(ctx.content.into());
    ctx.preview.set_preview_file_info("".into());
    let path = ctx.content.to_string();
    let ui_fin = ctx.preview.as_weak();
    let generation = ctx.request_generation;
    let generation_cell = ctx.generation;
    tokio::spawn(async move {
        let info = tokio::task::spawn_blocking(move || scan_folder_info(&path)).await;
        let Ok(info) = info else {
            return;
        };
        let _ = ui_fin.upgrade_in_event_loop(move |preview| {
            // Ignore the result if a newer request has taken over the popup.
            if generation_cell.load(Ordering::Relaxed) != generation {
                return;
            }
            preview.set_preview_file_info(info.into());
        });
    });
}

/// Scan a directory for an entry count, total size and the latest file
/// modification date. Separated so the blocking work can be unit-tested
/// without the window/tokio plumbing. Returns an empty string when `path`
/// is not a directory.
fn scan_folder_info(path: &str) -> String {
    let path = Path::new(path);
    if !path.is_dir() {
        return String::new();
    }
    let mut count = 0u64;
    let mut total_size = 0u64;
    let mut latest_mtime: Option<std::time::SystemTime> = None;
    if let Ok(entries) = std::fs::read_dir(path) {
        for entry in entries.flatten() {
            if count >= FOLDER_SCAN_MAX_ENTRIES {
                break;
            }
            count += 1;
            if let Ok(meta) = entry.metadata() {
                total_size += meta.len();
                if let Ok(mtime) = meta.modified() {
                    latest_mtime = Some(latest_mtime.map_or(mtime, |t| t.max(mtime)));
                }
            }
        }
    }
    let size_str = crate::stats_ui::format_size(total_size);
    let item_label = if count == 1 { "item" } else { "items" };
    let count_str = if count >= FOLDER_SCAN_MAX_ENTRIES {
        format!("{count}+")
    } else {
        count.to_string()
    };
    let date_str = latest_mtime.map(format_mtime).unwrap_or_default();
    format!("{count_str} {item_label} · {size_str} · {date_str}")
}

/// Preview for a copied file whose contents cannot be shown (archive, video,
/// audio): the path plus size and last-modified time.
fn show_file_preview(ctx: &PreviewContext) {
    ctx.preview.set_preview_clip_type(ctx.clip_type.into());
    ctx.preview.set_preview_text(ctx.content.into());
    ctx.preview.set_preview_file_info("".into());
    spawn_file_info(ctx, ctx.content.to_string());
}

/// Longest text handed to the hover tooltip. Slint lays out the whole string;
/// a multi-MB HTML clip otherwise allocates tens of MB for a tooltip.
const PREVIEW_TEXT_MAX_BYTES: usize = 4096;

fn bounded_preview(text: &str) -> String {
    if text.len() <= PREVIEW_TEXT_MAX_BYTES {
        return text.to_string();
    }
    let end = text
        .char_indices()
        .take_while(|(i, c)| *i + c.len_utf8() <= PREVIEW_TEXT_MAX_BYTES)
        .last()
        .map(|(i, c)| i + c.len_utf8())
        .unwrap_or(0);
    format!("{}…", &text[..end])
}

/// Preview for every other clip type (plain text, paths copied as text): show
/// the text and no file info.
fn show_text_preview(ctx: &PreviewContext) {
    ctx.preview.set_preview_clip_type(ctx.clip_type.into());
    ctx.preview
        .set_preview_text(bounded_preview(ctx.content).into());
}

/// Preview for a color clip: the parsed colour as a full-size swatch plus the
/// colour string. The swatch background comes from the clip content instead of
/// the placeholder grey the popup defaults to. Falls back to transparent on a
/// parse failure, mirroring the list-row swatch in thumbnail_cache::convert.
fn show_color_preview(ctx: &PreviewContext) {
    ctx.preview.set_preview_clip_type("color".into());
    ctx.preview
        .set_preview_text(bounded_preview(ctx.content).into());
    let color = cliptoo_core::color::ColorParser::try_parse(ctx.content)
        .map(|c| slint::Color::from_argb_u8(c.a, c.r, c.g, c.b))
        .unwrap_or(slint::Color::from_argb_u8(0, 0, 0, 0));
    ctx.preview.set_preview_color(color);
}

/// Preview for RTF clips: show the stripped plain text, not raw markup.
fn show_rtf_preview(ctx: &PreviewContext) {
    ctx.preview.set_preview_clip_type(ctx.clip_type.into());
    let stripped = cliptoo_core::content::strip_rtf(ctx.content);
    ctx.preview
        .set_preview_text(bounded_preview(&stripped).into());
}

/// Preview for HTML clips: show the stripped plain text, not raw markup.
fn show_html_preview(ctx: &PreviewContext) {
    ctx.preview.set_preview_clip_type(ctx.clip_type.into());
    let stripped = cliptoo_core::content::strip_html(ctx.content);
    ctx.preview
        .set_preview_text(bounded_preview(&stripped).into());
}

/// Preview for a copied file that is expected to hold text (code, config,
/// data, plain text, documents, unknown/no extension). Reads the file's
/// contents and previews them after the path and a size/last-modified line
/// when the content decodes as text, whatever its extension. Reading happens
/// on a blocking thread; large files are bounded and a binary file keeps the
/// path plus size/date view under its own file type.
fn show_text_file_preview(ctx: &PreviewContext) {
    // The path line is shown first so the popup is non-empty and the clip stays
    // identifiable even when the contents are truncated or unreadable.
    let path_line = ctx.content.to_string();
    let file_path = ctx.content.to_string();
    // Types that render the path as preview text get the size/date appended to
    // it; every other type shows it on its own info line.
    let joined_info = matches!(ctx.clip_type, "file_text" | "file_document");
    let fallback_type = ctx.clip_type.to_string();
    ctx.preview.set_preview_clip_type("file_text".into());
    ctx.preview.set_preview_text(path_line.clone().into());
    ctx.preview.set_preview_file_info("".into());

    let ui_fin = ctx.preview.as_weak();
    let generation = ctx.request_generation;
    let generation_cell = ctx.generation;
    tokio::spawn(async move {
        // Size/date and contents come from the same blocking stat/read pass so
        // the UI thread never touches the filesystem.
        let read = tokio::task::spawn_blocking(move || {
            let info = file_info(&file_path);
            (info, read_text_file_preview(&file_path))
        })
        .await;
        let Ok((info, read)) = read else {
            return;
        };
        let _ = ui_fin.upgrade_in_event_loop(move |preview_win| {
            // Ignore the result if a newer request has taken over the popup.
            if generation_cell.load(Ordering::Relaxed) != generation {
                return;
            }
            match read {
                Ok((contents, truncated)) => {
                    let separator = FILE_PREVIEW_SEPARATOR_GLYPH.repeat(FILE_PREVIEW_SEPARATOR_LEN);
                    let mut preview = path_line;
                    if !info.is_empty() {
                        preview.push('\n');
                        preview.push_str(&info);
                    }
                    preview.push('\n');
                    preview.push_str(&separator);
                    preview.push('\n');
                    preview.push_str(&contents);
                    if truncated {
                        preview.push_str("\n… (preview truncated)");
                    }
                    preview_win.set_preview_text(preview.into());
                }
                Err(_) => {
                    // Binary or unreadable file: path plus size/date, under the
                    // clip's own file type so its layout is unchanged.
                    preview_win.set_preview_clip_type(fallback_type.into());
                    if joined_info {
                        let mut preview = path_line;
                        if !info.is_empty() {
                            preview.push('\n');
                            preview.push_str(&info);
                        }
                        preview_win.set_preview_text(preview.into());
                    } else {
                        preview_win.set_preview_text(path_line.into());
                        preview_win.set_preview_file_info(info.into());
                    }
                }
            }
        });
    });
}

/// Read up to `FILE_TEXT_PREVIEW_MAX_BYTES` bytes from a text file. Returns the
/// decoded contents and whether it was truncated. Errors (missing path, no
/// permission, or binary content) signal the caller to keep the path-only view.
fn read_text_file_preview(path: &str) -> std::io::Result<(String, bool)> {
    let mut file = std::fs::File::open(path)?;
    let mut buf = Vec::with_capacity(FILE_TEXT_PREVIEW_MAX_BYTES);
    file.by_ref()
        .take(FILE_TEXT_PREVIEW_MAX_BYTES as u64 + 1)
        .read_to_end(&mut buf)?;
    let truncated = buf.len() > FILE_TEXT_PREVIEW_MAX_BYTES;
    buf.truncate(FILE_TEXT_PREVIEW_MAX_BYTES);
    // A NUL byte in the first chunk means this is almost certainly a binary
    // file, despite the text extension — don't render its bytes as text.
    if buf.contains(&0) {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "binary content",
        ));
    }
    Ok((String::from_utf8_lossy(&buf).into_owned(), truncated))
}

/// Uniform signature for a per-clip-type preview handler, so handlers can be
/// dispatched from a table keyed by clip type.
type PreviewHandler = fn(&PreviewContext<'_>);

/// Per-type preview handlers. Adding a new preview type only needs a new
/// `show_*_preview` function plus one entry here — the dispatcher itself
/// stays unchanged.
const PREVIEW_HANDLERS: &[(&str, PreviewHandler)] = &[
    ("code_snippet", show_code_preview),
    ("color", show_color_preview),
    ("link", show_link_preview),
    ("file_image", show_image_preview),
    // Text-capable file types read their contents; see `ClipType::is_text_file`
    // for the same set. Unknown/missing files fall back to the path view.
    ("file_text", show_text_file_preview),
    ("file_document", show_text_file_preview),
    ("file_generic", show_text_file_preview),
    ("file_dev", show_text_file_preview),
    ("file_danger", show_text_file_preview),
    ("file_data", show_text_file_preview),
    // Binary containers and media never hold text: no read, path + size/date.
    ("file_archive", show_file_preview),
    ("file_video", show_file_preview),
    ("file_audio", show_file_preview),
    ("folder", show_folder_preview),
    ("rtf", show_rtf_preview),
    ("html", show_html_preview),
];

pub fn setup_preview(
    app: &crate::AppWindow,
    preview: &crate::PreviewWindow,
    db: &Arc<cliptoo_core::db::DbPool>,
    dirs: &crate::app_dirs::AppDirs,
    preview_size: Arc<std::sync::atomic::AtomicU32>,
) {
    let preview_db = db.clone();
    let app_weak = app.as_weak();
    let preview_weak = preview.as_weak();
    let preview_fd = dirs.favicons_dir.clone();
    let preview_td = dirs.thumbnails_dir.clone();
    let preview_max_dim = preview_size.clone();
    app.on_request_preview(move |id: i32, x: f32, y: f32| {
        // Invalidate any still-running work from an earlier request or from a
        // preview that has since been dismissed.
        let generation = PREVIEW_GENERATION.fetch_add(1, Ordering::Relaxed) + 1;
        let db = preview_db.clone();
        let app = app_weak.clone();
        let preview = preview_weak.clone();
        let fav_dir = preview_fd.clone();
        let td = preview_td.clone();
        let max_dim = preview_max_dim.clone();
        tokio::spawn(async move {
            let result = db
                .with(|conn| cliptoo_core::db::queries::get_clip_type_and_content(conn, id as i64))
                .await;
            if let Ok((content, clip_type, content_hash)) = result {
                let _ = app.upgrade_in_event_loop(move |app| {
                    let Some(preview) = preview.upgrade() else {
                        return;
                    };
                    // A newer request (or a dismissal) superseded this one.
                    if PREVIEW_GENERATION.load(Ordering::Relaxed) != generation {
                        return;
                    }
                    let preview_max_dim = max_dim.load(Ordering::Relaxed);
                    let ctx = PreviewContext {
                        app: &app,
                        preview: &preview,
                        clip_type: &clip_type,
                        content: &content,
                        content_hash: &content_hash,
                        clip_id: id,
                        fav_dir: &fav_dir,
                        td: &td,
                        preview_max_dim,
                        request_generation: generation,
                        generation: &PREVIEW_GENERATION,
                    };
                    let handler = PREVIEW_HANDLERS
                        .iter()
                        .find(|(t, _)| *t == clip_type.as_str())
                        .map_or(show_text_preview as PreviewHandler, |(_, h)| *h);
                    handler(&ctx);
                    // Position after the content is set so the window can size
                    // itself before it becomes visible.
                    position_preview(&app, &preview, x, y);
                    let _ = preview.show();
                });
            }
        });
    });
}

pub fn setup_dismiss_preview(app: &crate::AppWindow, preview: &crate::PreviewWindow) {
    let preview = preview.as_weak();
    app.on_dismiss_preview(move || {
        // Also invalidate a request that is still waiting on the DB read: its
        // late `show()` would reopen the tooltip with no row left to dismiss
        // it (e.g. the pointer has already moved to the search box).
        PREVIEW_GENERATION.fetch_add(1, Ordering::Relaxed);
        if let Some(preview) = preview.upgrade() {
            park_preview(&preview);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::{FILE_TEXT_PREVIEW_MAX_BYTES, file_info, read_text_file_preview, scan_folder_info};
    use std::io::Write;

    fn temp_dir(label: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!("cliptoo_preview_{}_{}", std::process::id(), label))
    }

    fn temp_file(name: &str, bytes: &[u8]) -> std::path::PathBuf {
        let dir =
            std::env::temp_dir().join(format!("cliptoo_preview_{}_{}", std::process::id(), name));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("sample.txt");
        {
            let mut f = std::fs::File::create(&path).unwrap();
            f.write_all(bytes).unwrap();
        }
        path
    }

    #[test]
    fn reads_text_file_contents() {
        let path = temp_file("text", b"hello world\nsecond line\n");
        let (contents, truncated) = read_text_file_preview(path.to_str().unwrap()).unwrap();
        assert!(!truncated);
        assert_eq!(contents, "hello world\nsecond line\n");
        std::fs::remove_dir_all(path.parent().unwrap()).unwrap();
    }

    #[test]
    fn rejects_binary_content() {
        let path = temp_file("bin", b"PK\x03\x04\x00\x00stuff");
        assert!(read_text_file_preview(path.to_str().unwrap()).is_err());
        std::fs::remove_dir_all(path.parent().unwrap()).unwrap();
    }

    #[test]
    fn truncates_large_files_and_marks_it() {
        let path = temp_file("large", &vec![b'x'; FILE_TEXT_PREVIEW_MAX_BYTES + 100]);
        let (contents, truncated) = read_text_file_preview(path.to_str().unwrap()).unwrap();
        assert!(truncated);
        assert_eq!(contents.len(), FILE_TEXT_PREVIEW_MAX_BYTES);
        std::fs::remove_dir_all(path.parent().unwrap()).unwrap();
    }

    #[test]
    fn missing_file_errors() {
        assert!(read_text_file_preview("/definitely/not/a/file.txt").is_err());
    }

    #[test]
    fn folder_info_counts_entries() {
        let dir = temp_dir("folder_counts");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("a.txt"), b"hello").unwrap();
        std::fs::write(dir.join("b.txt"), b"world").unwrap();
        let info = scan_folder_info(dir.to_str().unwrap());
        assert!(info.starts_with("2 items · 10 B"), "got: {info:?}");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn folder_info_single_entry_is_singular() {
        let dir = temp_dir("folder_single");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("a.txt"), b"x").unwrap();
        let info = scan_folder_info(dir.to_str().unwrap());
        assert!(info.starts_with("1 item · 1 B"), "got: {info:?}");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn folder_info_empty_dir() {
        let dir = temp_dir("folder_empty");
        std::fs::create_dir_all(&dir).unwrap();
        let info = scan_folder_info(dir.to_str().unwrap());
        assert!(info.starts_with("0 items · 0 B"), "got: {info:?}");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn folder_info_non_dir_is_empty() {
        assert_eq!(scan_folder_info("/definitely/not/a/dir"), "");
    }

    #[test]
    fn file_info_prints_size_and_mtime() {
        let path = temp_file("info", b"hello");
        let info = file_info(path.to_str().unwrap());
        assert!(info.starts_with("5 B · "), "got: {info:?}");
        assert_eq!(info.matches('·').count(), 1, "got: {info:?}");
        std::fs::remove_dir_all(path.parent().unwrap()).unwrap();
    }

    #[test]
    fn file_info_is_empty_for_missing_paths_and_dirs() {
        assert_eq!(file_info("/definitely/not/a/file.txt"), "");
        let dir = temp_dir("info_dir");
        std::fs::create_dir_all(&dir).unwrap();
        assert_eq!(file_info(dir.to_str().unwrap()), "");
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
