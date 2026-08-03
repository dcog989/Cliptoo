use anyhow::Result;
use cliptoo_core::content::classifier::ContentProcessor;
use cliptoo_core::db::DbPool;
use cliptoo_core::db::queries::insert_or_bump;
use cliptoo_core::image::HASH_FILENAME_PREFIX_LEN;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tracing::{debug, info};
use wl_clipboard_rs::paste::{ClipboardType, Error as WlError, Seat, get_mime_types_ordered};

use crate::paste::PasteSuppressionSet;

use super::ClipboardPayload;
use super::is_blacklisted;
use super::reader::poll_clipboard;
use crate::helpers::refresh_clips;

#[allow(clippy::too_many_arguments)]
pub async fn run_listener(
    db: Arc<DbPool>,
    ui: slint::Weak<crate::AppWindow>,
    thumbnails_dir: PathBuf,
    favicons_dir: PathBuf,
    images_dir: PathBuf,
    suppression: Arc<PasteSuppressionSet>,
    blacklisted_apps: Vec<String>,
    preview_max_dim: u32,
    active_filter_state: Arc<std::sync::Mutex<String>>,
) -> Result<()> {
    let mut last_text_hash: Option<String> = None;
    let mut last_image_hash: Option<String> = None;
    let mut last_file_hash: Option<String> = None;
    let mut last_mime_types: Option<Vec<String>> = None;
    let mut last_full_read: Option<Instant> = None;
    const POLL_INTERVAL: Duration = Duration::from_millis(500);
    const FULL_READ_INTERVAL: Duration = Duration::from_secs(5);

    // True until the clipboard's state at startup has been observed. Content
    // already present when Cliptoo starts (left over from before launch) must
    // not be ingested as a "new" clip; only changes after startup count.
    let mut baseline = true;

    loop {
        let mime_types = match tokio::task::spawn_blocking(|| {
            get_mime_types_ordered(ClipboardType::Regular, Seat::Unspecified)
        })
        .await
        {
            Ok(Ok(mt)) => Some(mt),
            Ok(Err(WlError::ClipboardEmpty | WlError::NoSeats)) => {
                // An empty clipboard at startup seeds the baseline too.
                baseline = false;
                last_text_hash = None;
                last_image_hash = None;
                last_file_hash = None;
                last_mime_types = None;
                last_full_read = None;
                tokio::time::sleep(POLL_INTERVAL).await;
                continue;
            }
            Ok(Err(e)) => {
                tracing::error!("MIME type check: {e}");
                tokio::time::sleep(POLL_INTERVAL).await;
                continue;
            }
            Err(e) => {
                tracing::error!("spawn_blocking: {e}");
                tokio::time::sleep(POLL_INTERVAL).await;
                continue;
            }
        };

        let changed = last_mime_types.as_ref() != mime_types.as_ref();
        let stale = last_full_read.is_none_or(|t| t.elapsed() >= FULL_READ_INTERVAL);

        if !changed && !stale {
            tokio::time::sleep(POLL_INTERVAL).await;
            continue;
        }

        // A clipboard exposing text/uri-list is a real file/folder copy. The
        // path it also offers as text/plain is only an accessory representation,
        // so path-like text read from it (e.g. on a stale re-read, where the
        // uri-list has already been deduped) must not become a separate
        // `file_path` clip alongside the real Folder/file_* clip.
        let has_uri_list = mime_types
            .as_deref()
            .is_some_and(|mt| mt.iter().any(|m| m == "text/uri-list"));

        last_mime_types = mime_types;
        last_full_read = Some(Instant::now());

        let result = poll_clipboard(
            &mut last_text_hash,
            &mut last_image_hash,
            &mut last_file_hash,
        )
        .await;

        match result {
            Ok(Some(payload)) => {
                if baseline {
                    // First read after startup: seed the change-detection
                    // hashes (already updated by the reader) and skip ingest.
                    baseline = false;
                    debug!("clipboard: baseline captured; awaiting first change");
                    continue;
                }
                let sup_hash = match &payload {
                    ClipboardPayload::Text { sup_hash, .. }
                    | ClipboardPayload::FileUri { sup_hash, .. }
                    | ClipboardPayload::Image { sup_hash, .. } => *sup_hash,
                };

                if sup_hash != 0 && suppression.check_and_remove(sup_hash) {
                    info!("suppressed re-ingest of own paste");
                    continue;
                }

                match payload {
                    ClipboardPayload::Text { hash, content, .. } => {
                        // Classification (trim, hash, preview) is O(n) on the
                        // content; run it on the blocking pool so a large paste
                        // doesn't stall the runtime.
                        let classified = match tokio::task::spawn_blocking(move || {
                            ContentProcessor::process(&content, false)
                        })
                        .await
                        {
                            Ok(c) => c,
                            Err(e) => {
                                tracing::error!("classification task failed: {e}");
                                continue;
                            }
                        };
                        if let Some(classified) = classified {
                            if has_uri_list
                                && cliptoo_core::content::ContentProcessor::looks_like_path(
                                    &classified.content,
                                )
                            {
                                debug!("clipboard: path text on a text/uri-list clipboard skipped");
                                continue;
                            }

                            let source_app = crate::source_app::detect_source_app().await;

                            if is_blacklisted(source_app.as_deref(), &blacklisted_apps) {
                                debug!("blacklisted app {source_app:?} — skipping text clip");
                                continue;
                            }

                            let inserted = db
                                .with(|conn| {
                                    let inserted = insert_or_bump(
                                        conn,
                                        &classified.content,
                                        &classified.preview_content,
                                        &classified.content_hash,
                                        classified.clip_type.as_str(),
                                        source_app.as_deref(),
                                        classified.was_trimmed,
                                        classified.has_leading_whitespace,
                                        classified.is_multiline,
                                        classified.size_in_bytes,
                                        false,
                                    )?;
                                    if inserted {
                                        cliptoo_core::stats::increment_stat(
                                            conn,
                                            "UniqueClipsEver",
                                        )?;
                                    }
                                    Ok(inserted)
                                })
                                .await?;

                            if inserted {
                                let sa = source_app.as_deref().unwrap_or("unknown");
                                info!(
                                    "new clip: {} — {:?} (from {sa})",
                                    &hash[..12],
                                    classified.clip_type
                                );
                            } else {
                                info!("existing clip updated: {} — text", &hash[..12]);
                            }

                            let filter = active_filter_state.lock().unwrap().clone();
                            refresh_clips(
                                &db,
                                &ui,
                                &thumbnails_dir,
                                &favicons_dir,
                                "",
                                &filter,
                                None,
                            )
                            .await;
                        }
                    }
                    ClipboardPayload::FileUri { hash, content, .. } => {
                        let source_app = crate::source_app::detect_source_app().await;

                        if is_blacklisted(source_app.as_deref(), &blacklisted_apps) {
                            debug!("blacklisted app {source_app:?} — skipping file-uri clip");
                            continue;
                        }

                        let (classified, content) = match tokio::task::spawn_blocking(move || {
                            let classified = ContentProcessor::process(&content, true);
                            (classified, content)
                        })
                        .await
                        {
                            Ok(pair) => pair,
                            Err(e) => {
                                tracing::error!("classification task failed: {e}");
                                continue;
                            }
                        };

                        // `classified` is always `Some`: the reader rejects
                        // empty uri-list payloads, and `process` only returns
                        // `None` for empty content. Guard so a future reader
                        // change can't silently store a bogus clip.
                        let Some(c) = classified else {
                            tracing::error!("file-uri clip classified as empty; skipping");
                            continue;
                        };

                        let inserted = {
                            let (clip_type, preview_content, size, is_multiline) = (
                                c.clip_type.as_str().to_string(),
                                c.preview_content.clone(),
                                c.size_in_bytes,
                                c.is_multiline,
                            );
                            db.with(|conn| {
                                let ins = insert_or_bump(
                                    conn,
                                    &content,
                                    &preview_content,
                                    &hash,
                                    &clip_type,
                                    source_app.as_deref(),
                                    false,
                                    false,
                                    is_multiline,
                                    size,
                                    true,
                                )?;
                                if ins {
                                    cliptoo_core::stats::increment_stat(conn, "UniqueClipsEver")?;
                                }
                                Ok((ins, clip_type))
                            })
                            .await?
                        };

                        if inserted.0 {
                            let clip_type = &inserted.1;
                            let thumb_handle = if clip_type == "file_image" {
                                let path = std::path::Path::new(&content).to_owned();
                                let hash_c = hash.clone();
                                let td = thumbnails_dir.clone();
                                Some(tokio::task::spawn_blocking(move || {
                                    if let Err(e) =
                                        cliptoo_core::image::store_both_thumbnails_for_file(
                                            &td,
                                            &hash_c,
                                            &path,
                                            preview_max_dim,
                                        )
                                    {
                                        tracing::error!("store_both_thumbnails_for_file: {e}");
                                    }
                                }))
                            } else {
                                None
                            };

                            info!("new file-uri clip: {} — {clip_type}", &hash[..12]);
                            if let Some(h) = thumb_handle {
                                let _ = h.await;
                            }
                        }
                        let filter = active_filter_state.lock().unwrap().clone();
                        refresh_clips(&db, &ui, &thumbnails_dir, &favicons_dir, "", &filter, None)
                            .await;
                    }
                    ClipboardPayload::Image { hash, data, .. } => {
                        let source_app = crate::source_app::detect_source_app().await;

                        if is_blacklisted(source_app.as_deref(), &blacklisted_apps) {
                            debug!("blacklisted app {source_app:?} — skipping image clip");
                            continue;
                        }

                        let content_str = images_dir
                            .join(format!("{}.png", &hash[..HASH_FILENAME_PREFIX_LEN]))
                            .to_string_lossy()
                            .to_string();
                        let preview = format!("clipboard-image-{}.png", &hash[..12]);
                        let size = data.len() as i64;

                        let inserted = db
                            .with(|conn| {
                                let ins = insert_or_bump(
                                    conn,
                                    &content_str,
                                    &preview,
                                    &hash,
                                    "file_image",
                                    source_app.as_deref(),
                                    false,
                                    false,
                                    false,
                                    size,
                                    false,
                                )?;
                                if ins {
                                    cliptoo_core::stats::increment_stat(conn, "UniqueClipsEver")?;
                                }
                                Ok(ins)
                            })
                            .await?;

                        if inserted {
                            cliptoo_core::image::store_image(&images_dir, &hash, &data)?;
                            cliptoo_core::image::store_both_thumbnails(
                                &thumbnails_dir,
                                &hash,
                                &data,
                                preview_max_dim,
                            )?;
                            info!("new image clip: {} ({} bytes)", &hash[..12], size);
                        } else {
                            info!("existing image clip updated: {}", &hash[..12]);
                        }

                        let filter = active_filter_state.lock().unwrap().clone();
                        refresh_clips(&db, &ui, &thumbnails_dir, &favicons_dir, "", &filter, None)
                            .await;
                    }
                }
            }
            Ok(None) => {
                // A readable but content-free clipboard (or no change) also
                // means the startup state has been observed.
                baseline = false;
            }
            Err(e) => tracing::error!("Clipboard poll error: {e}"),
        }

        tokio::time::sleep(POLL_INTERVAL).await;
    }
}
