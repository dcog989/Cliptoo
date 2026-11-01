use anyhow::Result;
use cliptoo_core::content::hash::normalize_line_endings;
use sha2::Digest;
use std::io::Read;
use wl_clipboard_rs::paste::{ClipboardType, Error as WlError, MimeType, Seat, get_contents};

use super::ClipboardPayload;

const IMAGE_MIME_TYPES: &[&str] = &[
    "image/png",
    "image/jpeg",
    "image/bmp",
    "image/webp",
    "image/tiff",
    "image/gif",
];

/// Largest text (or rich-text) payload accepted from the clipboard. Anything
/// larger is ignored: reading it, normalising it and classifying it would hold
/// several copies in memory, and any app can write the clipboard without focus.
const MAX_TEXT_BYTES: usize = 16 * 1024 * 1024;
/// Largest `text/uri-list` payload. A list of paths never needs more.
const MAX_URI_LIST_BYTES: usize = 1024 * 1024;
/// Largest image payload accepted. Decoding also allocates width×height×4.
const MAX_IMAGE_BYTES: usize = 64 * 1024 * 1024;

/// Per-rendition hashes of the last read, used to tell a new copy from a stale
/// re-read of the same selection. Reset together when the MIME set changes so
/// every rendition is re-read; adding a rendition means adding one field here,
/// not a sixth variable to remember to clear.
#[derive(Default)]
pub(super) struct LastHashes {
    text: Option<String>,
    rtf: Option<String>,
    html: Option<String>,
    image: Option<String>,
    file: Option<String>,
}

impl LastHashes {
    pub(super) fn reset(&mut self) {
        *self = Self::default();
    }
}

/// Outcome of probing one clipboard rendition.
enum Probe {
    /// Not on the clipboard (or empty, or the read raced a change).
    Absent,
    /// On the clipboard with the same hash as the last read: the selection is
    /// unchanged. The caller must stop — falling through to a lower-priority
    /// rendition would ingest the accessory `text/plain` of the same selection
    /// (a file's path, an image's URL/alt text) as a clip of its own.
    Unchanged,
    /// New content to ingest.
    Found(ClipboardPayload),
}

/// Read one rendition from the clipboard, capped at `max_bytes`.
///
/// `None` means the clipboard offers no such rendition (or is empty, or the
/// read raced a change). The flag is true when the payload was longer than the
/// cap; one byte past it is read so an oversized payload is detectable.
async fn read_rendition(
    mime: MimeType<'static>,
    max_bytes: usize,
) -> Result<Option<(Vec<u8>, bool)>> {
    let result = tokio::task::spawn_blocking(move || {
        get_contents(ClipboardType::Regular, Seat::Unspecified, mime)
    })
    .await
    .map_err(|e| anyhow::anyhow!("spawn_blocking: {e}"))?;

    let (reader, _mime) = match result {
        Ok(r) => r,
        Err(WlError::NoSeats | WlError::ClipboardEmpty | WlError::NoMimeType) => return Ok(None),
        Err(e) => return Err(e.into()),
    };

    let mut bytes = Vec::new();
    reader
        .take((max_bytes + 1) as u64)
        .read_to_end(&mut bytes)?;
    let oversized = bytes.len() > max_bytes;
    Ok(Some((bytes, oversized)))
}

/// Read a text-family payload (`text/plain` or `text/rtf`) and return a
/// [`ClipboardPayload::Text`] when the content is new and non-empty. Shared by
/// the plain-text and RTF probes; the classifier detects RTF from the content
/// itself (`is_rtf`).
async fn try_text_payload(
    mime: MimeType<'static>,
    last_hash: &mut Option<String>,
) -> Result<Probe> {
    let Some((raw_bytes, oversized)) = read_rendition(mime, MAX_TEXT_BYTES).await? else {
        return Ok(Probe::Absent);
    };

    // Decode lossily: cutting the stream can split a UTF-8 character.
    let raw = String::from_utf8(raw_bytes)
        .unwrap_or_else(|e| String::from_utf8_lossy(e.as_bytes()).into_owned());

    let normalized = normalize_line_endings(&raw);
    let (hash, sup_hash) = cliptoo_core::content::hash::sha256_hex_and_prefix(&normalized);

    if last_hash.as_deref() == Some(&hash) {
        return Ok(Probe::Unchanged);
    }
    *last_hash = Some(hash.clone());

    if oversized {
        tracing::warn!("clipboard: text payload over {MAX_TEXT_BYTES} bytes ignored");
        return Ok(Probe::Unchanged);
    }

    if normalized.trim().is_empty() {
        return Ok(Probe::Absent);
    }

    Ok(Probe::Found(ClipboardPayload::Text {
        hash,
        content: normalized,
        sup_hash,
    }))
}

async fn try_text(last_hash: &mut Option<String>) -> Result<Probe> {
    try_text_payload(MimeType::Text, last_hash).await
}

/// Read the raw `text/rtf` payload, if offered. Rich-text producers (e.g.
/// LibreOffice) expose the RTF markup under this MIME type and a plain-text
/// rendition under `text/plain`; polling `text/rtf` first is what lets the
/// classifier record an `Rtf` clip instead of a lossy `Text` one. The payload
/// reuses `ClipboardPayload::Text` — the classifier detects RTF from the
/// content itself (`is_rtf`).
async fn try_rtf(last_hash: &mut Option<String>) -> Result<Probe> {
    try_text_payload(MimeType::Specific("text/rtf"), last_hash).await
}

/// Read the raw `text/html` payload, if offered. Browsers and office suites
/// expose rich text as `text/html` (with or without a `text/rtf` rendition);
/// polling it before `text/plain` lets the classifier record an `Html` clip
/// instead of a lossy `Text` one. The payload reuses
/// `ClipboardPayload::Text` — the classifier detects HTML from the content
/// itself (`is_html`).
async fn try_html(last_hash: &mut Option<String>) -> Result<Probe> {
    try_text_payload(MimeType::Specific("text/html"), last_hash).await
}

async fn try_file_uri_list(last_hash: &mut Option<String>) -> Result<Probe> {
    let Some((raw_bytes, oversized)) =
        read_rendition(MimeType::Specific("text/uri-list"), MAX_URI_LIST_BYTES).await?
    else {
        return Ok(Probe::Absent);
    };

    let raw = String::from_utf8_lossy(&raw_bytes);

    let content = raw
        .lines()
        .filter_map(|line| line.strip_prefix("file://"))
        .map(cliptoo_core::content::percent_decode_path)
        .collect::<Vec<_>>()
        .join("\n");

    if content.is_empty() {
        return Ok(Probe::Absent);
    }

    let (hash, sup_hash) = cliptoo_core::content::hash::sha256_hex_and_prefix(&content);

    if last_hash.as_deref() == Some(&hash) {
        return Ok(Probe::Unchanged);
    }
    *last_hash = Some(hash);

    if oversized {
        tracing::warn!("clipboard: uri-list over {MAX_URI_LIST_BYTES} bytes ignored");
        return Ok(Probe::Unchanged);
    }

    Ok(Probe::Found(ClipboardPayload::FileUri {
        content,
        sup_hash,
    }))
}

/// Probe the image type the clipboard advertises first. Each probe opens its
/// own Wayland transfer, so an app that offers only `image/webp` should not pay
/// for the png/jpeg/bmp attempts ahead of it. The fixed list stays as the
/// fallback for a stale MIME list.
fn image_mimes_in_probe_order(mime_types: Option<&[String]>) -> Vec<&'static str> {
    let advertised =
        |mime: &&str| mime_types.is_some_and(|types| types.iter().any(|t| t.as_str() == *mime));
    let mut ordered = IMAGE_MIME_TYPES.to_vec();
    ordered.sort_by_key(|mime| !advertised(mime));
    ordered
}

async fn try_image(last_hash: &mut Option<String>, mime_types: Option<&[String]>) -> Result<Probe> {
    for &mime_str in &image_mimes_in_probe_order(mime_types) {
        let Some((data, oversized)) =
            read_rendition(MimeType::Specific(mime_str), MAX_IMAGE_BYTES).await?
        else {
            continue;
        };

        if data.is_empty() {
            continue;
        }

        let digest = sha2::Sha256::digest(&data);
        let hash = const_hex::encode(digest);

        if last_hash.as_deref() == Some(&hash) {
            return Ok(Probe::Unchanged);
        }
        *last_hash = Some(hash.clone());

        if oversized {
            tracing::warn!("clipboard: image payload over {MAX_IMAGE_BYTES} bytes ignored");
            return Ok(Probe::Unchanged);
        }

        return Ok(Probe::Found(ClipboardPayload::Image {
            hash,
            data,
            mime: mime_str.to_string(),
        }));
    }

    Ok(Probe::Absent)
}

pub(super) async fn poll_clipboard(
    hashes: &mut LastHashes,
    mime_types: Option<&[String]>,
    probe_images: bool,
) -> Result<Option<ClipboardPayload>> {
    // File copies first: a file manager (e.g. Dolphin) exposes the copied
    // path both as text/uri-list AND text/plain. If plain text were read
    // first, a copied folder/file would be captured as a text clip and end
    // up as a `file_path` clip instead of its real `Folder`/`file_*` type.
    // An *unchanged* uri-list stops the read: falling through would ingest
    // that accessory path text, which hashes to the same row and downgrades
    // the file clip to `file_path`, dropping its thumbnail.
    match try_file_uri_list(&mut hashes.file).await? {
        Probe::Found(payload) => return Ok(Some(payload)),
        Probe::Unchanged => return Ok(None),
        Probe::Absent => {}
    }
    // RTF before plain text: a rich-text producer offers both the RTF markup
    // (text/rtf) and a plain rendition (text/plain). Reading the markup first
    // records an `Rtf` clip; reading text/plain first would lose the markup.
    // An unchanged RTF is the same selection, so its plain rendition must not
    // be ingested either.
    match try_rtf(&mut hashes.rtf).await? {
        Probe::Found(payload) => return Ok(Some(payload)),
        Probe::Unchanged => return Ok(None),
        Probe::Absent => {}
    }
    // Image before plain text when the clipboard advertises an image type: a
    // web-page image copy offers image/* alongside a non-empty text/plain
    // (the image URL or alt text). Reading text first would spawn a spurious
    // Link/Text clip and push the image ingest off to the next stale re-read.
    // `probe_images` gates the (potentially multi-MB) download on a mime change
    // or the slow image recheck so an unchanged image isn't re-read on every
    // cheap text stale-read.
    let offers_image =
        mime_types.is_some_and(|mt| mt.iter().any(|m| IMAGE_MIME_TYPES.contains(&m.as_str())));
    if probe_images && offers_image {
        match try_image(&mut hashes.image, mime_types).await? {
            Probe::Found(payload) => return Ok(Some(payload)),
            Probe::Unchanged => return Ok(None),
            Probe::Absent => {}
        }
    }
    // RTF and HTML are alternative renditions of the same rich-text selection;
    // a producer offering both (e.g. LibreOffice, Word) has its Rtf clip
    // ingested by the earlier RTF probe, so the HTML rendition must not spawn
    // a duplicate Html clip on the next stale re-read.
    let offers_rtf = mime_types.is_some_and(|mt| mt.iter().any(|m| m == "text/rtf"));
    // HTML after the image probe, gated on the clipboard NOT advertising an
    // image or RTF: a browser image copy offers image/* alongside text/html +
    // the image's URL/alt text, and a rich-text copy offers text/rtf
    // alongside text/html. Reading the HTML first would record a spurious
    // Html clip and defer the image/RTF ingest — the same priority the image
    // and RTF probes already assert over text/plain. (An HTML payload is
    // text-sized, so it is polled on every read like text/rtf, not gated on
    // probe_images.)
    if !offers_image && !offers_rtf {
        match try_html(&mut hashes.html).await? {
            Probe::Found(payload) => return Ok(Some(payload)),
            Probe::Unchanged => return Ok(None),
            Probe::Absent => {}
        }
    }
    if let Probe::Found(payload) = try_text(&mut hashes.text).await? {
        return Ok(Some(payload));
    }
    // Fallback probe for a stale/mismatched mime list whose payload offers an
    // image type the advertised list omits. Fails fast (NoMimeType) on
    // text-only clipboards. Skipped when the advertised list already matched
    // (just probed above) so the payload isn't downloaded twice.
    if probe_images
        && !offers_image
        && let Probe::Found(payload) = try_image(&mut hashes.image, mime_types).await?
    {
        return Ok(Some(payload));
    }
    Ok(None)
}
