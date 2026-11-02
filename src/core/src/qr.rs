//! QR decoding for image clips.
//!
//! Decoding is on demand (context menu), never part of the capture path, so an
//! image clip costs nothing until the user asks for its code.

use std::path::Path;

use anyhow::{Context, Result};

/// Longest side handed to the detector. rqrr's grid search is linear in pixels,
/// so a full 4K screenshot would otherwise pin a blocking thread for seconds.
const MAX_DECODE_DIM: u32 = 2048;

/// Most codes shown for one image. A screen full of codes is not a clipboard
/// use case; the cap keeps the result window bounded.
const MAX_CODES: usize = 8;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QrCode {
    /// Stable kind key ("url", "wifi", "email", "phone", "contact", "text");
    /// the UI owns the translated label.
    pub kind: &'static str,
    pub payload: String,
}

/// Decode every QR code in the image file at `path`.
///
/// Fails only when the file cannot be opened or decoded as an image. An image
/// without a QR code yields an empty list.
pub fn decode_path(path: &Path) -> Result<Vec<QrCode>> {
    let img = image::open(path).with_context(|| format!("opening image {}", path.display()))?;
    let mut codes = decode_image(&img);
    if codes.is_empty() && img.width().max(img.height()) > MAX_DECODE_DIM {
        // A small code inside a large screenshot can be lost at full size
        // (rqrr's detector works best at screen scale); retry downscaled.
        codes = decode_image(&img.thumbnail(MAX_DECODE_DIM, MAX_DECODE_DIM));
    }
    codes.truncate(MAX_CODES);
    Ok(codes)
}

fn decode_image(image: &image::DynamicImage) -> Vec<QrCode> {
    let gray = image.to_luma8();
    let mut codes = scan(&gray);
    if codes.is_empty() {
        // White-on-dark codes are common on dark-themed screens; rqrr only
        // detects dark modules on a light background, so retry inverted.
        let mut inverted = gray;
        for pixel in inverted.pixels_mut() {
            pixel[0] = u8::MAX - pixel[0];
        }
        codes = scan(&inverted);
    }
    codes
}

fn scan(gray: &image::GrayImage) -> Vec<QrCode> {
    let mut prepared = rqrr::PreparedImage::prepare(gray.clone());
    prepared
        .detect_grids()
        .into_iter()
        .filter_map(|grid| grid.decode().ok())
        .map(|(_, payload)| QrCode {
            kind: kind_key(&payload),
            payload,
        })
        .collect()
}

fn kind_key(payload: &str) -> &'static str {
    let payload = payload.trim_start();
    if payload.starts_with("http://") || payload.starts_with("https://") {
        "url"
    } else if payload.starts_with("WIFI:") {
        "wifi"
    } else if payload.starts_with("mailto:") {
        "email"
    } else if payload.starts_with("tel:") {
        "phone"
    } else if payload.starts_with("BEGIN:VCARD") {
        "contact"
    } else {
        "text"
    }
}

#[cfg(test)]
mod tests {
    use super::kind_key;

    #[test]
    fn payload_kind_keys() {
        assert_eq!(kind_key("https://example.com"), "url");
        assert_eq!(kind_key("  http://example.com"), "url");
        assert_eq!(kind_key("WIFI:S:net;T:WPA;P:secret;;"), "wifi");
        assert_eq!(kind_key("mailto:a@b.c"), "email");
        assert_eq!(kind_key("tel:+123"), "phone");
        assert_eq!(kind_key("BEGIN:VCARD\nVERSION:3.0"), "contact");
        assert_eq!(kind_key("just text"), "text");
        // A URL scheme is not enough: only http(s) is opened by the UI.
        assert_eq!(kind_key("file:///etc/passwd"), "text");
    }

    #[test]
    fn image_without_code_decodes_to_empty() {
        let blank = image::DynamicImage::ImageLuma8(image::GrayImage::from_pixel(
            64,
            64,
            image::Luma([255]),
        ));
        assert!(super::decode_image(&blank).is_empty());
    }
}
