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
    let (hash, sup_hash) = cliptoo_core::content::hash::sha256_hex_and_prefix(&normalized);

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
        sup_hash,
    }))
}

/// Read the raw `text/rtf` payload, if offered. Rich-text producers (e.g.
/// LibreOffice) expose the RTF markup under this MIME type and a plain-text
/// rendition under `text/plain`; polling `text/rtf` first is what lets the
/// classifier record an `Rtf` clip instead of a lossy `Text` one. The payload
/// reuses `ClipboardPayload::Text` — the classifier detects RTF from the
/// content itself (`is_rtf`).
async fn try_rtf(last_hash: &mut Option<String>) -> Result<Option<ClipboardPayload>> {
    let result = tokio::task::spawn_blocking(|| {
        get_contents(
            ClipboardType::Regular,
            Seat::Unspecified,
            MimeType::Specific("text/rtf"),
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

    let normalized = normalize_line_endings(&raw);
    let (hash, sup_hash) = cliptoo_core::content::hash::sha256_hex_and_prefix(&normalized);

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
        sup_hash,
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

    let (hash, sup_hash) = cliptoo_core::content::hash::sha256_hex_and_prefix(&content);

    if last_hash.as_deref() == Some(&hash) {
        return Ok(None);
    }
    *last_hash = Some(hash.clone());

    Ok(Some(ClipboardPayload::FileUri { content, sup_hash }))
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

        let digest = sha2::Sha256::digest(&data);
        let hash = const_hex::encode(digest);
        // Same convention as `cliptoo_core::content::hash::sha256_hex_and_prefix`:
        // first 8 bytes of the digest, little-endian, for the fast in-memory
        // paste-suppression set. Previously hard-coded to 0, which meant a
        // self-pasted image could never be recognised as our own paste.
        let sup_hash = u64::from_le_bytes(
            digest[..8]
                .try_into()
                .expect("SHA-256 output is >= 8 bytes"),
        );

        if last_hash.as_deref() == Some(&hash) {
            return Ok(None);
        }
        *last_hash = Some(hash.clone());

        return Ok(Some(ClipboardPayload::Image {
            hash,
            data,
            sup_hash,
        }));
    }

    Ok(None)
}

pub(super) async fn poll_clipboard(
    last_text_hash: &mut Option<String>,
    last_rtf_hash: &mut Option<String>,
    last_image_hash: &mut Option<String>,
    last_file_hash: &mut Option<String>,
    mime_types: Option<&[String]>,
) -> Result<Option<ClipboardPayload>> {
    // File copies first: a file manager (e.g. Dolphin) exposes the copied
    // path both as text/uri-list AND text/plain. If plain text were read
    // first, a copied folder/file would be captured as a text clip and end
    // up as a `file_path` clip instead of its real `Folder`/`file_*` type.
    if let Some(payload) = try_file_uri_list(last_file_hash).await? {
        return Ok(Some(payload));
    }
    // RTF before plain text: a rich-text producer offers both the RTF markup
    // (text/rtf) and a plain rendition (text/plain). Reading the markup first
    // records an `Rtf` clip; reading text/plain first would lose the markup.
    if let Some(payload) = try_rtf(last_rtf_hash).await? {
        return Ok(Some(payload));
    }
    // Image before plain text when the clipboard advertises an image type: a
    // web-page image copy offers image/* alongside a non-empty text/plain
    // (the image URL or alt text). Reading text first would spawn a spurious
    // Link/Text clip and push the image ingest off to the next stale re-read.
    let offers_image =
        mime_types.is_some_and(|mt| mt.iter().any(|m| IMAGE_MIME_TYPES.contains(&m.as_str())));
    if offers_image && let Some(payload) = try_image(last_image_hash).await? {
        return Ok(Some(payload));
    }
    if let Some(payload) = try_text(last_text_hash).await? {
        return Ok(Some(payload));
    }
    // Fallback probe for a stale/mismatched mime list whose payload offers an
    // image type the advertised list omits. Fails fast (NoMimeType) on
    // text-only clipboards, so this stays cheap on the common path.
    try_image(last_image_hash).await
}
