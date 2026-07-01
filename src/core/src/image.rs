use anyhow::{Context, Result};
use std::io::Cursor;
use std::path::{Path, PathBuf};

const THUMB_MAX_DIM: u32 = 36;
// PREVIEW_FALLBACK_DIM is used by generate_both_thumbnails (the in-memory path
// that has no access to settings). The disk-write path accepts preview_max_dim
// as a parameter so the user's hover_image_preview_size setting is honoured.
pub const PREVIEW_FALLBACK_DIM: u32 = 400;
pub const HASH_FILENAME_PREFIX_LEN: usize = 16;

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
    let (w, h) = (img.width() as f64, img.height() as f64);
    if w.max(h) <= max_dim as f64 {
        return img;
    }
    let scale = max_dim as f64 / w.max(h);
    let nw = (w * scale).round().max(1.0) as u32;
    let nh = (h * scale).round().max(1.0) as u32;
    img.resize_exact(nw, nh, image::imageops::FilterType::Triangle)
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

/// Load stored full-resolution image bytes from disk by content hash.
pub fn load_image(dir: &Path, hash: &str) -> Result<Vec<u8>> {
    let path = dir.join(format!("{}.png", &hash[..HASH_FILENAME_PREFIX_LEN]));
    Ok(std::fs::read(path)?)
}

/// Check if a stored full-resolution image exists for the given hash.
pub fn image_exists(dir: &Path, hash: &str) -> bool {
    dir.join(format!("{}.png", &hash[..HASH_FILENAME_PREFIX_LEN]))
        .exists()
}

// ── Thumbnail store (WebP, thumbnails_dir) ────────────────────────────────────

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

    let need_thumb = !thumb_path.exists();
    let need_preview = !preview_path.exists();

    if !need_thumb && !need_preview {
        return Ok(());
    }

    let img = decode_image(data)?;

    if need_thumb && need_preview {
        let mut file = std::fs::File::create(&thumb_path)?;
        resize_to(img.clone(), THUMB_MAX_DIM).write_to(&mut file, image::ImageFormat::WebP)?;
        let mut file = std::fs::File::create(&preview_path)?;
        resize_to(img, preview_max_dim).write_to(&mut file, image::ImageFormat::WebP)?;
    } else if need_thumb {
        let mut file = std::fs::File::create(&thumb_path)?;
        resize_to(img, THUMB_MAX_DIM).write_to(&mut file, image::ImageFormat::WebP)?;
    } else if need_preview {
        let mut file = std::fs::File::create(&preview_path)?;
        resize_to(img, preview_max_dim).write_to(&mut file, image::ImageFormat::WebP)?;
    }

    Ok(())
}

// ── In-memory thumbnail generation ───────────────────────────────────────────

/// Decode once and return both `(thumb_webp, preview_webp)` bytes without
/// touching disk. Prefer this over two separate `generate_thumbnail` calls
/// when both sizes are needed in-memory.
pub fn generate_both_thumbnails(data: &[u8]) -> Result<(Vec<u8>, Vec<u8>)> {
    let img = decode_image(data)?;
    let mut thumb_buf = Cursor::new(Vec::new());
    resize_to(img.clone(), THUMB_MAX_DIM).write_to(&mut thumb_buf, image::ImageFormat::WebP)?;
    let mut preview_buf = Cursor::new(Vec::new());
    resize_to(img, PREVIEW_FALLBACK_DIM).write_to(&mut preview_buf, image::ImageFormat::WebP)?;
    Ok((thumb_buf.into_inner(), preview_buf.into_inner()))
}

/// Generate a single thumbnail at `max_dim` and return WebP bytes.
/// For callers that need both sizes, prefer [`generate_both_thumbnails`].
pub fn generate_thumbnail(data: &[u8], max_dim: u32) -> Result<Vec<u8>> {
    let img = decode_image(data)?;
    let mut buf = Cursor::new(Vec::new());
    resize_to(img, max_dim).write_to(&mut buf, image::ImageFormat::WebP)?;
    Ok(buf.into_inner())
}

// ── Pixel data ────────────────────────────────────────────────────────────────

/// Decode image bytes into raw RGBA pixel data. Supports all formats including JXL.
pub fn decode_to_rgba(data: &[u8]) -> Result<(Vec<u8>, u32, u32)> {
    let img = decode_image(data)?;
    let rgba = img.to_rgba8();
    let (w, h) = rgba.dimensions();
    Ok((rgba.into_raw(), w, h))
}
