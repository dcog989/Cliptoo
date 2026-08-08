use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

use crate::icon;

const THUMB_MAX_DIM: u32 = 36;
// PREVIEW_FALLBACK_DIM is the default preview dimension for callers without
// access to settings. The disk-write path accepts preview_max_dim as a
// parameter so the user's hover_image_preview_size setting is honoured.
pub const PREVIEW_FALLBACK_DIM: u32 = 400;
pub const HASH_FILENAME_PREFIX_LEN: usize = 16;

/// Canonical list of all image-file extensions.  Single source of truth for
/// `filetype.rs` (DB `ClipType` classification) and `preview.rs` (hover popup
/// `resolved_type`).  Keep this in sync with the two downstream consumers.
pub const IMAGE_EXTENSIONS: &[&str] = &[
    "png", "jpg", "jpeg", "gif", "webp", "svg", "avif", "heic", "jxl", "ico", "bmp", "tiff", "tif",
    "psd", "xcf", "raw", "arw", "cr2", "nef", "dng",
];

/// Subset of [`IMAGE_EXTENSIONS`] that the `image` crate + jxl-oxide cannot
/// decode.  These get a placeholder thumbnail instead of failing silently.
const UNDECODABLE_IMAGE_EXTS: &[&str] = &["psd", "xcf", "raw", "arw", "cr2", "nef", "dng"];

/// Resolution at which SVG content is rasterised in `decode_image` when
/// the raw SVG byte stream is pasted (e.g. from a web app).  The file-
/// backed path (`store_both_thumbnails_for_file`) copies SVG files as-is
/// so Slint can render them natively at any size.
const SVG_RENDER_SIZE: u32 = 1024;

// ── Directory layout ──────────────────────────────────────────────────────────
//
// Full-resolution clipboard images are stored as PNG under `images_dir`:
//   `{images_dir}/{hash[..HASH_FILENAME_PREFIX_LEN]}.png`
//
// Thumbnails are stored as WebP under `thumbnails_dir` (a different path):
//   list-cell:  `{thumbnails_dir}/{hash[..HASH_FILENAME_PREFIX_LEN]}.webp`
//   preview:    `{thumbnails_dir}/{hash[..HASH_FILENAME_PREFIX_LEN]}_preview.webp`
//
// The two directories MUST be distinct so that `prune_cache` in maintenance.rs
// (which only receives `thumbnails_dir`) never touches the full-res PNGs.
// Callers in `clipboard.rs` pass `images_dir` to `store_image` and
// `thumbnails_dir` to `store_both_thumbnails`, satisfying this invariant.

// ── Decode ────────────────────────────────────────────────────────────────────

fn decode_image(data: &[u8]) -> Result<image::DynamicImage> {
    image::load_from_memory(data).or_else(|_| {
        // SVG rasterization via resvg/usvg/tiny-skia
        if let Ok((rgba, w, h)) = icon::rasterize_svg(data, SVG_RENDER_SIZE)
            && let Some(img) = image::RgbaImage::from_raw(w, h, rgba)
        {
            return Ok(image::DynamicImage::ImageRgba8(img));
        }
        // JXL via jxl-oxide
        let jxl = jxl_oxide::JxlImage::builder()
            .read(std::io::Cursor::new(data))
            .map_err(|e| anyhow::anyhow!("jxl-oxide decode: {e}"))?;
        let render = jxl
            .render_frame(0)
            .map_err(|e| anyhow::anyhow!("jxl render: {e}"))?;
        let fb = render.image_all_channels();
        let w = fb.width() as u32;
        let h = fb.height() as u32;
        let c = fb.channels();
        let raw: Vec<u8> = fb
            .buf()
            .iter()
            .map(|&v| (v.clamp(0.0, 1.0) * 255.0).round() as u8)
            .collect();
        let rgba = match c {
            0..=2 => return Err(anyhow::anyhow!("unsupported JXL channel count {c}")),
            3 => {
                let mut buf = Vec::with_capacity((w * h * 4) as usize);
                for ch in raw.chunks(3) {
                    buf.extend_from_slice(&[ch[0], ch[1], ch[2], 255]);
                }
                image::RgbaImage::from_raw(w, h, buf).context("jxl rgba")?
            }
            4 => image::RgbaImage::from_raw(w, h, raw).context("jxl rgba")?,
            _ => return Err(anyhow::anyhow!("unsupported JXL channel count {c}")),
        };
        Ok(image::DynamicImage::ImageRgba8(rgba))
    })
}

fn resize_to(img: image::DynamicImage, max_dim: u32) -> image::DynamicImage {
    let (w, h) = (img.width(), img.height());
    if w.max(h) <= max_dim {
        return img;
    }
    // Aspect-preserving downscale that fits within max_dim × max_dim.
    img.thumbnail(max_dim, max_dim)
}

// ── Full-resolution store (PNG, images_dir) ───────────────────────────────────

/// Store full-resolution clipboard image to disk as PNG, keyed by content-hash
/// prefix. Writes to `images_dir`, which must be distinct from `thumbnails_dir`.
pub fn store_image(dir: &Path, hash: &str, data: &[u8]) -> Result<PathBuf> {
    let path = dir.join(format!("{}.png", &hash[..HASH_FILENAME_PREFIX_LEN]));
    if path.exists() {
        return Ok(path);
    }
    let img = decode_image(data)?;
    img.save(&path)?;
    Ok(path)
}

// ── Thumbnail store (WebP, thumbnails_dir) ────────────────────────────────────

/// Write the list-cell and preview thumbnails as WebP, skipping any file that
/// already exists. `render(dim)` produces the image for a given target
/// dimension and is only called for sizes that actually get written.
fn write_thumbnails_pair(
    thumb_path: &Path,
    preview_path: &Path,
    thumb_dim: u32,
    preview_dim: u32,
    render: impl Fn(u32) -> image::DynamicImage,
) -> Result<()> {
    if !thumb_path.exists() {
        let mut file = std::fs::File::create(thumb_path)?;
        render(thumb_dim).write_to(&mut file, image::ImageFormat::WebP)?;
    }
    if !preview_path.exists() {
        let mut file = std::fs::File::create(preview_path)?;
        render(preview_dim).write_to(&mut file, image::ImageFormat::WebP)?;
    }
    Ok(())
}

/// Decode once and write both list-cell (36px) and preview thumbnails
/// as WebP to `thumbnails_dir`. This is the sole thumbnail write path.
///
/// `preview_max_dim` is the max pixel dimension for the preview image;
/// pass `settings.hover_image_preview_size` (default 300) here.
pub fn store_both_thumbnails(
    dir: &Path,
    hash: &str,
    data: &[u8],
    preview_max_dim: u32,
) -> Result<()> {
    let thumb_path = dir.join(format!("{}.webp", &hash[..HASH_FILENAME_PREFIX_LEN]));
    let preview_path = dir.join(format!(
        "{}_preview.webp",
        &hash[..HASH_FILENAME_PREFIX_LEN]
    ));

    // Skip decoding entirely when both files already exist.
    if thumb_path.exists() && preview_path.exists() {
        return Ok(());
    }

    let img = decode_image(data)?;
    write_thumbnails_pair(
        &thumb_path,
        &preview_path,
        THUMB_MAX_DIM,
        preview_max_dim,
        |dim| resize_to(img.clone(), dim),
    )
}

/// Check whether `ext` (lowercased) is one of the IMAGE-classified formats
/// that cannot be decoded by the `image` crate or jxl-oxide.
pub fn is_undecodable_image_ext(ext: &str) -> bool {
    UNDECODABLE_IMAGE_EXTS.contains(&ext)
}

// ── Placeholder thumbnail ────────────────────────────────────────────────────
//
// Formats like PSD, XCF, and camera raw are classified as FileImage but cannot
// be decoded.  Rather than failing silently, we write a gentle gradient
// placeholder so the list cell and preview popup always have something to show.

fn make_placeholder(size: u32) -> image::DynamicImage {
    let mut img = image::RgbaImage::new(size, size);
    for y in 0..size {
        for x in 0..size {
            let v = (x as f64 / size as f64 * 30.0 + y as f64 / size as f64 * 20.0) as u8;
            img.put_pixel(
                x,
                y,
                image::Rgba([
                    160u8.saturating_add(v),
                    170u8.saturating_add(v),
                    185u8.saturating_add(v),
                    255,
                ]),
            );
        }
    }
    image::DynamicImage::ImageRgba8(img)
}

// ── File-aware thumbnail store ───────────────────────────────────────────────

/// Write thumbnails for a `FileImage` clip backed by an on-disk file.
///
/// * **SVG** – the file is copied directly to `{hash}.svg` / `{hash}_preview.svg`
///   so Slint can load it natively. No decode attempt.
/// * **PSD, XCF, RAW, …** – a generic gradient placeholder is written as WebP
///   since the `image` crate cannot decode these.
/// * **All other formats** – decoded and written as WebP (same as
///   [`store_both_thumbnails`]).
pub fn store_both_thumbnails_for_file(
    dir: &Path,
    hash: &str,
    file_path: &Path,
    preview_max_dim: u32,
) -> Result<()> {
    let prefix = &hash[..hash.len().min(HASH_FILENAME_PREFIX_LEN)];

    let thumb_webp = dir.join(format!("{prefix}.webp"));
    let thumb_svg = dir.join(format!("{prefix}.svg"));
    let preview_webp = dir.join(format!("{prefix}_preview.webp"));
    let preview_svg = dir.join(format!("{prefix}_preview.svg"));

    let ext = file_path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_lowercase());

    match ext.as_deref() {
        Some("svg") => {
            if !thumb_svg.exists() {
                std::fs::copy(file_path, &thumb_svg)?;
            }
            if !preview_svg.exists() {
                std::fs::copy(file_path, &preview_svg)?;
            }
            Ok(())
        }
        Some(e) if UNDECODABLE_IMAGE_EXTS.contains(&e) => write_thumbnails_pair(
            &thumb_webp,
            &preview_webp,
            THUMB_MAX_DIM,
            preview_max_dim,
            make_placeholder,
        ),
        _ => {
            let data = std::fs::read(file_path)?;
            store_both_thumbnails(dir, hash, &data, preview_max_dim)
        }
    }
}
