use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

use crate::icon;

const THUMB_MAX_DIM: u32 = 36;
pub const HASH_FILENAME_PREFIX_LEN: usize = 16;

/// Preview thumbnails are written at this multiple of the logical preview size:
/// the popup is laid out in logical px but Slint rasterises it at device px, so
/// a 1× preview is upscaled and blurred on a HiDPI display.
pub const PREVIEW_PIXEL_RATIO: u32 = 2;

/// Records the pixel ratio the cached previews were written at, so previews
/// from a build with a different ratio are dropped once. See
/// [`migrate_preview_ratio`].
const PREVIEW_RATIO_MARKER: &str = ".preview_ratio";

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
// (which is handed `thumbnails_dir` and `favicons_dir`, never `images_dir`)
// never touches the full-res PNGs.
// Callers in `clipboard.rs` pass `images_dir` to `store_image` and
// `thumbnails_dir` to `store_both_thumbnails`, satisfying this invariant.

// ── Decode ────────────────────────────────────────────────────────────────────

fn decode_image(data: &[u8]) -> Result<image::DynamicImage> {
    if let Some(img) = decode_raster(data) {
        return Ok(img);
    }
    decode_vector_or_jxl(data)
}

/// Largest image side accepted from a clipboard, file or download. A payload
/// under the byte cap can still declare gigapixel dimensions, so these limits
/// are what actually protects the process.
const MAX_DECODE_DIM: u32 = 16_384;
/// Largest decoded buffer accepted, in bytes (a 16k x 4k RGBA frame fits).
const MAX_DECODE_ALLOC: u64 = 256 * 1024 * 1024;

fn decode_limits() -> image::Limits {
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(MAX_DECODE_DIM);
    limits.max_image_height = Some(MAX_DECODE_DIM);
    limits.max_alloc = Some(MAX_DECODE_ALLOC);
    limits
}

/// Decode a raster image (PNG/JPEG/WebP/...) under the limits above. Returns
/// `None` for an undecodable image or one over them, so a decompression bomb
/// cannot allocate its declared dimensions.
pub fn decode_raster(data: &[u8]) -> Option<image::DynamicImage> {
    let mut reader = image::ImageReader::new(std::io::Cursor::new(data))
        .with_guessed_format()
        .ok()?;
    reader.limits(decode_limits());
    reader.decode().ok()
}

fn decode_vector_or_jxl(data: &[u8]) -> Result<image::DynamicImage> {
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
}

fn resize_to(img: image::DynamicImage, max_dim: u32) -> image::DynamicImage {
    let (w, h) = (img.width(), img.height());
    if w.max(h) <= max_dim {
        return img;
    }
    // Aspect-preserving downscale that fits within max_dim × max_dim. Lanczos3
    // rather than `thumbnail`'s triangle filter: the preview is drawn at device
    // pixels, where a cheap filter shows as soft text and edges.
    img.resize(max_dim, max_dim, image::imageops::FilterType::Lanczos3)
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
        render(preview_dim.saturating_mul(PREVIEW_PIXEL_RATIO))
            .write_to(&mut file, image::ImageFormat::WebP)?;
    }
    Ok(())
}

/// Decode once and write both list-cell (36px) and preview thumbnails
/// as WebP to `thumbnails_dir`. This is the sole thumbnail write path.
///
/// `preview_max_dim` is the logical max dimension for the preview image; pass
/// `settings.hover_image_preview_size` (default 300) here. The stored preview
/// is [`PREVIEW_PIXEL_RATIO`]× that, in device pixels.
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

/// Drop cached preview thumbnails written at a different [`PREVIEW_PIXEL_RATIO`]
/// so the next hover regenerates them sharp (a 1× preview is blurred on HiDPI).
/// Resolution-independent SVG previews and the 36px list-cell thumbnails are
/// kept. Runs once per ratio change; the marker makes later starts a no-op.
pub fn migrate_preview_ratio(thumbnails_dir: &Path) {
    let marker = thumbnails_dir.join(PREVIEW_RATIO_MARKER);
    let ratio = PREVIEW_PIXEL_RATIO.to_string();
    if std::fs::read_to_string(&marker).unwrap_or_default().trim() == ratio {
        return;
    }
    if let Ok(entries) = std::fs::read_dir(thumbnails_dir) {
        for entry in entries.filter_map(|e| e.ok()) {
            if entry
                .file_name()
                .to_string_lossy()
                .ends_with("_preview.webp")
                && let Err(e) = std::fs::remove_file(entry.path())
            {
                tracing::warn!("migrate_preview_ratio: {:?}: {e}", entry.path());
            }
        }
    }
    if let Err(e) = std::fs::write(&marker, ratio) {
        tracing::warn!("migrate_preview_ratio: {marker:?}: {e}");
    }
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
/// * **SVG** – a valid file is copied directly to `{hash}.svg` /
///   `{hash}_preview.svg` so Slint can load it natively. An invalid or empty
///   SVG falls back to the placeholder, like the undecodable formats below.
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
            let data = std::fs::read(file_path)?;
            // Copy a valid SVG verbatim so Slint renders it natively (crisp at
            // any size). A 0-byte or malformed file (e.g. an empty placeholder
            // committed by mistake) must not be cached: Slint would later fail
            // to load it with "document does not have a root node". Fall back
            // to the placeholder used for undecodable formats and drop any
            // stale invalid SVG pair so it can't shadow the placeholder.
            if !icon::svg_is_valid(&data) {
                let _ = std::fs::remove_file(&thumb_svg);
                let _ = std::fs::remove_file(&preview_svg);
                return write_thumbnails_pair(
                    &thumb_webp,
                    &preview_webp,
                    THUMB_MAX_DIM,
                    preview_max_dim,
                    make_placeholder,
                );
            }
            if !thumb_svg.exists() {
                std::fs::write(&thumb_svg, &data)?;
            }
            if !preview_svg.exists() {
                std::fs::write(&preview_svg, &data)?;
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

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(label: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("cliptoo_image_{}_{label}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn png_bytes(w: u32, h: u32) -> Vec<u8> {
        let mut out = std::io::Cursor::new(Vec::new());
        image::RgbImage::from_pixel(w, h, image::Rgb([10, 40, 90]))
            .write_to(&mut out, image::ImageFormat::Png)
            .unwrap();
        out.into_inner()
    }

    #[test]
    fn decode_limits_reject_oversized_images() {
        // Garbage decodes to nothing rather than panicking.
        assert!(decode_raster(b"not an image").is_none());
        let png = png_bytes(2, 2);
        assert!(decode_raster(&png).is_some());
        // The guards that stop a decompression bomb must stay in place.
        let limits = decode_limits();
        assert_eq!(limits.max_image_width, Some(MAX_DECODE_DIM));
        assert_eq!(limits.max_image_height, Some(MAX_DECODE_DIM));
        assert_eq!(limits.max_alloc, Some(MAX_DECODE_ALLOC));
    }

    #[test]
    fn preview_thumbnail_is_stored_at_the_device_pixel_ratio() {
        let dir = temp_dir("ratio");
        let data = png_bytes(1200, 600);
        let hash = "0123456789abcdef0123";
        store_both_thumbnails(&dir, hash, &data, 300).unwrap();

        let preview = dir.join(format!(
            "{}_preview.webp",
            &hash[..HASH_FILENAME_PREFIX_LEN]
        ));
        let (pw, ph) = image::image_dimensions(&preview).unwrap();
        assert_eq!(pw.max(ph), 300 * PREVIEW_PIXEL_RATIO, "preview: {pw}x{ph}");

        let thumb = dir.join(format!("{}.webp", &hash[..HASH_FILENAME_PREFIX_LEN]));
        let (tw, th) = image::image_dimensions(&thumb).unwrap();
        assert!(tw.max(th) <= THUMB_MAX_DIM, "thumb: {tw}x{th}");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn migrate_ratio_drops_stale_previews_once() {
        let dir = temp_dir("migrate");
        let thumb = dir.join("0123456789abcdef.webp");
        let stale = dir.join("0123456789abcdef_preview.webp");
        let svg = dir.join("0123456789abcdef_preview.svg");
        std::fs::write(&thumb, b"x").unwrap();
        std::fs::write(&stale, b"x").unwrap();
        std::fs::write(&svg, b"x").unwrap();

        migrate_preview_ratio(&dir);
        assert!(!stale.exists(), "stale preview must be dropped");
        assert!(thumb.exists(), "list-cell thumbnail must survive");
        assert!(svg.exists(), "SVG preview is resolution-independent");

        // Second start is a no-op: previews written since must survive.
        let fresh = dir.join("fedcba9876543210_preview.webp");
        std::fs::write(&fresh, b"y").unwrap();
        migrate_preview_ratio(&dir);
        assert!(fresh.exists(), "fresh preview must survive a restart");
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
