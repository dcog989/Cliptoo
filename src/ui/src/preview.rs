use std::io::Read;
use std::path::Path;
use std::sync::Arc;

use slint::{ComponentHandle, Model};

use cliptoo_core::image::{HASH_FILENAME_PREFIX_LEN, PREVIEW_FALLBACK_DIM};

use crate::helpers;

const CODE_PREVIEW_WIDTH: f32 = 560.0;
const DEFAULT_PREVIEW_WIDTH: f32 = 400.0;
const POPUP_MARGIN: f32 = 8.0;
const POPUP_OFFSET_X: f32 = 20.0;
/// Maximum bytes read from a copied file for its text preview, so a huge file
/// (e.g. a multi-GB log) is never slurped into memory just to render a tooltip.
const FILE_TEXT_PREVIEW_MAX_BYTES: usize = 64 * 1024;

/// Everything a preview handler needs, bundled so the per-type handlers can
/// share one uniform `fn(&PreviewContext)` signature and be dispatched from a
/// table instead of a growing `match`.
struct PreviewContext<'a> {
    ui: &'a crate::AppWindow,
    clip_type: &'a str,
    content: &'a str,
    content_hash: &'a str,
    clip_id: i32,
    fav_dir: &'a Path,
    td: &'a Path,
}

/// Position the preview popup next to the pointer, clamped inside the window.
fn position_popup(ui: &crate::AppWindow, clip_type: &str, x: f32, y: f32) {
    let popup_w: f32 = if clip_type == "code_snippet" {
        CODE_PREVIEW_WIDTH
    } else {
        DEFAULT_PREVIEW_WIDTH
    };
    let window_w = ui.window().size().width as f32;
    let scale = ui.window().scale_factor();
    let window_w_logical = window_w / scale;
    let max_x = (window_w_logical - popup_w - POPUP_MARGIN).max(POPUP_MARGIN);
    let popup_x = (x + POPUP_OFFSET_X).clamp(POPUP_MARGIN, max_x);
    ui.set_preview_popup_x(popup_x);
    ui.set_preview_popup_y(y);
}

/// Preview for a code-snippet clip: the snippet text in a fixed-width popup.
fn show_code_preview(ctx: &PreviewContext) {
    ctx.ui.set_preview_clip_type("code_snippet".into());
    ctx.ui.set_preview_text(ctx.content.into());
}

/// Preview for a link clip: the URL plus the cached or fetched page title and
/// favicon, updating the row in-place when the favicon arrives.
fn show_link_preview(ctx: &PreviewContext) {
    let ui = ctx.ui;
    ui.set_preview_clip_type("link".into());
    ui.set_preview_text(ctx.content.into());
    ui.set_preview_favicon(slint::Image::default());
    ui.set_preview_web_title("".into());
    let c = ctx.content.to_string();
    let fd = ctx.fav_dir.to_path_buf();
    let clip_id = ctx.clip_id;
    let dark = crate::theme::cached_resolved_theme().0;
    let w = ui.as_weak();
    if let Some(t) = crate::favicon::load_cached_page_title(&c, &fd) {
        ui.set_preview_web_title(t.into());
    }
    tokio::spawn(async move {
        let cached_title = crate::favicon::load_cached_page_title(&c, &fd);
        let (title, fav_path) = if cached_title.is_some() {
            (None, crate::favicon::fetch_favicon(&c, &fd, dark).await)
        } else {
            let (t, f) = tokio::join!(
                helpers::fetch_page_title(&c),
                crate::favicon::fetch_favicon(&c, &fd, dark),
            );
            if let Some(ref t) = t {
                crate::favicon::cache_page_title(&c, t, &fd);
            }
            (t, f)
        };
        let _ = w.upgrade_in_event_loop(move |ui| {
            if let Some(t) = title.or(cached_title) {
                ui.set_preview_web_title(t.into());
            }
            if let Some(p) = fav_path {
                let img = slint::Image::load_from_path(&p).unwrap_or_default();
                ui.set_preview_favicon(img.clone());
                let model = ui.get_clips();
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
        });
    });
}

/// Preview for a file-image clip: the stored preview WebP/SVG if present,
/// otherwise generate it in the background and load the result when ready.
fn show_image_preview(ctx: &PreviewContext) {
    let ui = ctx.ui;
    let content = ctx.content;
    let content_hash = ctx.content_hash;
    let td = ctx.td;
    let preview_webp = td.join(format!(
        "{}_preview.webp",
        &content_hash[..HASH_FILENAME_PREFIX_LEN]
    ));
    let preview_svg = td.join(format!(
        "{}_preview.svg",
        &content_hash[..HASH_FILENAME_PREFIX_LEN]
    ));
    if preview_webp.exists() {
        let img = slint::Image::load_from_path(&preview_webp).unwrap_or_default();
        ui.set_preview_image(img);
    } else if preview_svg.exists() {
        let img = slint::Image::load_from_path(&preview_svg).unwrap_or_default();
        ui.set_preview_image(img);
    } else {
        let file_path = content.to_string();
        let td2 = td.to_path_buf();
        let hash2 = content_hash.to_string();
        let w = ui.as_weak();
        tokio::spawn(async move {
            let _ = cliptoo_core::image::store_both_thumbnails_for_file(
                &td2,
                &hash2,
                Path::new(&file_path),
                PREVIEW_FALLBACK_DIM,
            );
            let p = td2.join(format!(
                "{}_preview.webp",
                &hash2[..HASH_FILENAME_PREFIX_LEN]
            ));
            if p.exists() {
                let _ = w.upgrade_in_event_loop(move |ui| {
                    let img = slint::Image::load_from_path(&p).unwrap_or_default();
                    ui.set_preview_image(img);
                });
            } else {
                let svg_p = td2.join(format!(
                    "{}_preview.svg",
                    &hash2[..HASH_FILENAME_PREFIX_LEN]
                ));
                if svg_p.exists() {
                    let _ = w.upgrade_in_event_loop(move |ui| {
                        let img = slint::Image::load_from_path(&svg_p).unwrap_or_default();
                        ui.set_preview_image(img);
                    });
                }
            }
        });
    }
    ui.set_preview_clip_type("file_image".into());
    ui.set_preview_text(content.into());
}

/// Preview for a folder clip: the path plus an entry count, total size and the
/// latest file modification date. The directory scan runs on a blocking thread
/// so a large folder (or slow network mount) can't stall the UI thread; the
/// path line is shown first so the popup is immediately non-empty.
fn show_folder_preview(ctx: &PreviewContext) {
    ctx.ui.set_preview_clip_type("folder".into());
    ctx.ui.set_preview_text(ctx.content.into());
    let path = ctx.content.to_string();
    let ui_fin = ctx.ui.as_weak();
    tokio::spawn(async move {
        let info = tokio::task::spawn_blocking(move || scan_folder_info(&path)).await;
        let Ok(info) = info else {
            return;
        };
        let _ = ui_fin.upgrade_in_event_loop(move |ui| {
            ui.set_preview_file_info(info.into());
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
    let mut latest_mtime = 0i64;
    if let Ok(entries) = std::fs::read_dir(path) {
        for entry in entries.flatten() {
            count += 1;
            if let Ok(meta) = entry.metadata() {
                total_size += meta.len();
                if let Ok(mtime) = meta.modified()
                    && let Ok(dur) = mtime.duration_since(std::time::UNIX_EPOCH)
                {
                    latest_mtime = latest_mtime.max(dur.as_secs() as i64);
                }
            }
        }
    }
    let size_str = if total_size < 1024 {
        format!("{total_size} B")
    } else if total_size < 1024 * 1024 {
        format!("{:.1} KB", total_size as f64 / 1024.0)
    } else if total_size < 1024 * 1024 * 1024 {
        format!("{:.1} MB", total_size as f64 / (1024.0 * 1024.0))
    } else {
        format!("{:.2} GB", total_size as f64 / (1024.0 * 1024.0 * 1024.0))
    };
    let item_label = if count == 1 { "item" } else { "items" };
    let date_str = if latest_mtime > 0 {
        chrono::DateTime::from_timestamp(latest_mtime, 0)
            .map(|dt| dt.format("%Y-%m-%d %H:%M").to_string())
            .unwrap_or_default()
    } else {
        String::new()
    };
    format!("{count} {item_label} · {size_str} · {date_str}")
}

/// Preview for every other clip type (text, file_*): show text.
fn show_text_preview(ctx: &PreviewContext) {
    ctx.ui.set_preview_clip_type(ctx.clip_type.into());
    ctx.ui.set_preview_text(ctx.content.into());
}

/// Preview for a color clip: the parsed colour as a full-size swatch plus the
/// colour string. The swatch background comes from the clip content instead of
/// the placeholder grey the popup defaults to. Falls back to transparent on a
/// parse failure, mirroring the list-row swatch in thumbnail_cache::convert.
fn show_color_preview(ctx: &PreviewContext) {
    ctx.ui.set_preview_clip_type("color".into());
    ctx.ui.set_preview_text(ctx.content.into());
    let color = cliptoo_core::color::ColorParser::try_parse(ctx.content)
        .map(|c| slint::Color::from_argb_u8(c.a, c.r, c.g, c.b))
        .unwrap_or(slint::Color::from_argb_u8(0, 0, 0, 0));
    ctx.ui.set_preview_color(color);
}

/// Preview for RTF clips: show the stripped plain text, not raw markup.
fn show_rtf_preview(ctx: &PreviewContext) {
    ctx.ui.set_preview_clip_type(ctx.clip_type.into());
    let stripped = cliptoo_core::content::strip_rtf(ctx.content);
    ctx.ui.set_preview_text(stripped.into());
}

/// Preview for a copied text document (`.txt`, `.md`, `.log`, …): read the
/// file's contents and preview them instead of just the path. Reading happens
/// on a blocking thread; large files are bounded and binary files fall back to
/// showing the path alone (same as a generic file clip).
fn show_text_file_preview(ctx: &PreviewContext) {
    // The path line is shown first so the popup is non-empty and the clip stays
    // identifiable even when the contents are truncated or unreadable.
    let path_line = ctx.content.to_string();
    let file_path = ctx.content.to_string();
    ctx.ui.set_preview_clip_type("file_text".into());
    ctx.ui.set_preview_text(path_line.clone().into());

    let ui_fin = ctx.ui.as_weak();
    tokio::spawn(async move {
        let read = tokio::task::spawn_blocking(move || read_text_file_preview(&file_path)).await;
        let Ok(Ok((contents, truncated))) = read else {
            return;
        };
        let mut preview = format!("{path_line}\n{}\n{contents}", "─".repeat(40));
        if truncated {
            preview.push_str("\n… (preview truncated)");
        }
        let _ = ui_fin.upgrade_in_event_loop(move |ui| {
            ui.set_preview_text(preview.into());
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
    ("file_text", show_text_file_preview),
    ("folder", show_folder_preview),
    ("rtf", show_rtf_preview),
];

pub fn setup_preview(
    ui: &crate::AppWindow,
    db: &Arc<cliptoo_core::db::DbPool>,
    dirs: &crate::app_dirs::AppDirs,
) {
    let preview_db = db.clone();
    let preview_ui = ui.as_weak();
    let preview_fd = dirs.favicons_dir.clone();
    let preview_td = dirs.thumbnails_dir.clone();
    ui.on_request_preview(move |id: i32, x: f32, y: f32| {
        let db = preview_db.clone();
        let ui = preview_ui.clone();
        let fav_dir = preview_fd.clone();
        let td = preview_td.clone();
        tokio::spawn(async move {
            let result = db
                .with(|conn| cliptoo_core::db::queries::get_clip_type_and_content(conn, id as i64))
                .await;
            if let Ok((content, clip_type, content_hash)) = result {
                let _ = ui.upgrade_in_event_loop(move |ui| {
                    position_popup(&ui, &clip_type, x, y);
                    let ctx = PreviewContext {
                        ui: &ui,
                        clip_type: &clip_type,
                        content: &content,
                        content_hash: &content_hash,
                        clip_id: id,
                        fav_dir: &fav_dir,
                        td: &td,
                    };
                    let handler = PREVIEW_HANDLERS
                        .iter()
                        .find(|(t, _)| *t == clip_type.as_str())
                        .map_or(show_text_preview as PreviewHandler, |(_, h)| *h);
                    handler(&ctx);
                    ui.set_preview_visible(true);
                });
            }
        });
    });
}

pub fn setup_dismiss_preview(ui: &crate::AppWindow) {
    let dismiss_ui = ui.as_weak();
    ui.on_dismiss_preview(move || {
        if let Some(ui) = dismiss_ui.upgrade() {
            ui.set_preview_visible(false);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::{FILE_TEXT_PREVIEW_MAX_BYTES, read_text_file_preview, scan_folder_info};
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
}
