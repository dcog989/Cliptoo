use anyhow::Result;
use cliptoo_core::content::hash::normalize_line_endings;
use sha2::Digest;
use std::io::Read;
use wl_clipboard_rs::paste::{ClipboardType, MimeType, Seat, get_contents};

use super::ClipboardPayload;

const IMAGE_MIME_TYPES: &[&str] = &[
    "image/png",
    "image/jpeg",
    "image/bmp",
    "image/webp",
    "image/tiff",
];

async fn try_text(last_hash: &mut Option<String>) -> Result<Option<ClipboardPayload>> {
    let result = tokio::task::spawn_blocking(|| {
        get_contents(ClipboardType::Regular, Seat::Unspecified, MimeType::Text)
    })
    .await
    .map_err(|e| anyhow::anyhow!("spawn_blocking: {e}"))?;

    let (mut reader, _mime) = match result {
        Ok(r) => r,
        Err(wl_clipboard_rs::paste::Error::NoSeats)
        | Err(wl_clipboard_rs::paste::Error::ClipboardEmpty)
        | Err(wl_clipboard_rs::paste::Error::NoMimeType) => return Ok(None),
        Err(e) => return Err(e.into()),
    };

    let mut raw = String::new();
    reader.read_to_string(&mut raw)?;

    let normalized = normalize_line_endings(&raw);
    let hash = cliptoo_core::content::hash::sha256_hex(&normalized);

    if last_hash.as_deref() == Some(&hash) {
        return Ok(None);
    }
    *last_hash = Some(hash.clone());

    if normalized.trim().is_empty() {
        return Ok(None);
    }

    Ok(Some(ClipboardPayload::Text {
        hash,
        content: normalized,
    }))
}

async fn try_file_uri_list(last_hash: &mut Option<String>) -> Result<Option<ClipboardPayload>> {
    let result = tokio::task::spawn_blocking(|| {
        get_contents(
            ClipboardType::Regular,
            Seat::Unspecified,
            MimeType::Specific("text/uri-list"),
        )
    })
    .await
    .map_err(|e| anyhow::anyhow!("spawn_blocking: {e}"))?;

    let (mut reader, _mime) = match result {
        Ok(r) => r,
        Err(wl_clipboard_rs::paste::Error::NoSeats)
        | Err(wl_clipboard_rs::paste::Error::ClipboardEmpty)
        | Err(wl_clipboard_rs::paste::Error::NoMimeType) => return Ok(None),
        Err(e) => return Err(e.into()),
    };

    let mut raw = String::new();
    reader.read_to_string(&mut raw)?;

    let content = raw
        .lines()
        .filter_map(|line| line.strip_prefix("file://"))
        .map(cliptoo_core::content::percent_decode_path)
        .collect::<Vec<_>>()
        .join("\n");

    if content.is_empty() {
        return Ok(None);
    }

    let hash = cliptoo_core::content::hash::sha256_hex(&content);

    if last_hash.as_deref() == Some(&hash) {
        return Ok(None);
    }
    *last_hash = Some(hash.clone());

    Ok(Some(ClipboardPayload::FileUri { hash, content }))
}

async fn try_image(last_hash: &mut Option<String>) -> Result<Option<ClipboardPayload>> {
    for &mime_str in IMAGE_MIME_TYPES {
        let result = tokio::task::spawn_blocking(move || {
            get_contents(
                ClipboardType::Regular,
                Seat::Unspecified,
                MimeType::Specific(mime_str),
            )
        })
        .await
        .map_err(|e| anyhow::anyhow!("spawn_blocking: {e}"))?;

        let (mut reader, _mime) = match result {
            Ok(r) => r,
            Err(wl_clipboard_rs::paste::Error::NoSeats)
            | Err(wl_clipboard_rs::paste::Error::ClipboardEmpty)
            | Err(wl_clipboard_rs::paste::Error::NoMimeType) => continue,
            Err(e) => return Err(e.into()),
        };

        let mut data = Vec::new();
        reader.read_to_end(&mut data)?;

        if data.is_empty() {
            continue;
        }

        let hash = const_hex::encode(sha2::Sha256::digest(&data));

        if last_hash.as_deref() == Some(&hash) {
            return Ok(None);
        }
        *last_hash = Some(hash.clone());

        return Ok(Some(ClipboardPayload::Image { hash, data }));
    }

    Ok(None)
}

pub(super) async fn poll_clipboard(
    last_text_hash: &mut Option<String>,
    last_image_hash: &mut Option<String>,
    last_file_hash: &mut Option<String>,
) -> Result<Option<ClipboardPayload>> {
    if let Some(payload) = try_text(last_text_hash).await? {
        return Ok(Some(payload));
    }
    if let Some(payload) = try_file_uri_list(last_file_hash).await? {
        return Ok(Some(payload));
    }
    try_image(last_image_hash).await
}
