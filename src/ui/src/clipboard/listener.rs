use anyhow::Result;
use cliptoo_core::content::classifier::ContentProcessor;
use cliptoo_core::db::DbPool;
use cliptoo_core::db::models::ClipType;
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

/// Read the current blacklist from shared state and test `source_app` against
/// it. The settings UI swaps the list on change, so edits take effect on the
/// next clipboard event without a restart.
fn is_blacklisted_live(
    state: &Arc<std::sync::Mutex<Vec<String>>>,
    source_app: Option<&str>,
) -> bool {
    let blacklist = state
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    is_blacklisted(source_app, &blacklist)
}

#[allow(clippy::too_many_arguments)]
pub async fn run_listener(
    db: Arc<DbPool>,
    ui: slint::Weak<crate::AppWindow>,
    thumbnails_dir: PathBuf,
    favicons_dir: PathBuf,
    images_dir: PathBuf,
    suppression: Arc<PasteSuppressionSet>,
    blacklist_state: Arc<std::sync::Mutex<Vec<String>>>,
    preview_max_dim: u32,
    active_filter_state: Arc<std::sync::Mutex<String>>,
) -> Result<()> {
    let mut last_text_hash: Option<String> = None;
    let mut last_rtf_hash: Option<String> = None;
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
                last_rtf_hash = None;
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

        // A clipboard exposing text/rtf is a rich-text copy; its text/plain
        // rendition is likewise accessory and must not become a separate Text
        // clip (the Rtf clip already carries the plain text via its preview).
        let has_rtf = mime_types
            .as_deref()
            .is_some_and(|mt| mt.iter().any(|m| m == "text/rtf"));

        last_mime_types = mime_types;
        last_full_read = Some(Instant::now());

        let result = poll_clipboard(
            &mut last_text_hash,
            &mut last_rtf_hash,
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

                            // The plain-text rendition of an RTF clipboard (read
                            // on a stale re-read, after the Rtf clip was already
                            // ingested via text/rtf) must not spawn a Text clip.
                            if has_rtf && classified.clip_type != ClipType::Rtf {
                                debug!("clipboard: plain text on a text/rtf clipboard skipped");
                                continue;
                            }

                            let source_app = crate::source_app::detect_source_app().await;

                            if is_blacklisted_live(&blacklist_state, source_app.as_deref()) {
                                debug!("blacklisted app {source_app:?} — skipping text clip");
                                continue;
                            }

                            // A failed insert (e.g. disk full) must not kill the
                            // listener loop; log and keep polling instead of
                            // propagating out of run_listener (same error
                            // containment as the file-uri and image branches).
                            let inserted = match insert_clip_with_stat(
                                &db,
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
                            )
                            .await
                            {
                                Ok(inserted) => inserted,
                                Err(e) => {
                                    tracing::error!("text clip insert failed: {e}");
                                    continue;
                                }
                            };

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
                    ClipboardPayload::FileUri { content, .. } => {
                        let source_app = crate::source_app::detect_source_app().await;

                        if is_blacklisted_live(&blacklist_state, source_app.as_deref()) {
                            debug!("blacklisted app {source_app:?} — skipping file-uri clip");
                            continue;
                        }

                        let classified = match tokio::task::spawn_blocking(move || {
                            ContentProcessor::process(&content, true)
                        })
                        .await
                        {
                            Ok(c) => c,
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

                        let inserted = match insert_clip_with_stat(
                            &db,
                            &c.content,
                            &c.preview_content,
                            &c.content_hash,
                            c.clip_type.as_str(),
                            source_app.as_deref(),
                            false,
                            false,
                            c.is_multiline,
                            c.size_in_bytes,
                            true,
                        )
                        .await
                        {
                            Ok(inserted) => inserted,
                            Err(e) => {
                                tracing::error!("file-uri clip insert failed: {e}");
                                continue;
                            }
                        };

                        if inserted {
                            let clip_type = c.clip_type.as_str();
                            let thumb_handle = if clip_type == "file_image" {
                                let path = std::path::Path::new(&c.content).to_owned();
                                let hash_c = c.content_hash.clone();
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

                            info!("new file-uri clip: {} — {clip_type}", &c.content_hash[..12]);
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

                        if is_blacklisted_live(&blacklist_state, source_app.as_deref()) {
                            debug!("blacklisted app {source_app:?} — skipping image clip");
                            continue;
                        }

                        let content_str = images_dir
                            .join(format!("{}.png", &hash[..HASH_FILENAME_PREFIX_LEN]))
                            .to_string_lossy()
                            .to_string();
                        let preview = format!("clipboard-image-{}.png", &hash[..12]);
                        let size = data.len() as i64;

                        // A failed insert (e.g. disk full) must not kill the
                        // listener loop; log and keep polling instead of
                        // propagating out of run_listener.
                        let inserted = match insert_clip_with_stat(
                            &db,
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
                        )
                        .await
                        {
                            Ok(inserted) => inserted,
                            Err(e) => {
                                tracing::error!("image clip insert failed: {e}");
                                continue;
                            }
                        };

                        if inserted {
                            let hash_prefix = hash[..12].to_string();
                            let images = images_dir.clone();
                            let thumbnails = thumbnails_dir.clone();
                            // Decode + disk writes are blocking; run on the
                            // blocking pool. A corrupt/undecodable image must
                            // not kill the listener loop either — same error
                            // containment as the file-uri thumbnail path.
                            let _ = tokio::task::spawn_blocking(move || {
                                if let Err(e) =
                                    cliptoo_core::image::store_image(&images, &hash, &data)
                                {
                                    tracing::error!("store_image: {e}");
                                }
                                if let Err(e) = cliptoo_core::image::store_both_thumbnails(
                                    &thumbnails,
                                    &hash,
                                    &data,
                                    preview_max_dim,
                                ) {
                                    tracing::error!("store_both_thumbnails: {e}");
                                }
                            })
                            .await;
                            info!("new image clip: {} ({} bytes)", hash_prefix, size);
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

/// Insert (or bump) a clip and, only when a genuinely new row was created,
/// record it in the "UniqueClipsEver" lifetime stat.
///
/// Shared ingest path for the text, file-uri and image listener branches;
/// callers supply the classification arguments themselves.
#[allow(clippy::too_many_arguments)]
async fn insert_clip_with_stat(
    db: &DbPool,
    content: &str,
    preview_content: &str,
    content_hash: &str,
    clip_type: &str,
    source_app: Option<&str>,
    was_trimmed: bool,
    has_leading_whitespace: bool,
    is_multiline: bool,
    size_in_bytes: i64,
    is_file_uri: bool,
) -> Result<bool> {
    db.with(|conn| {
        // Single transaction so a crash between the insert and the lifetime
        // stat bump cannot desync "UniqueClipsEver" from the stored rows.
        let tx = conn.unchecked_transaction()?;
        let inserted = insert_or_bump(
            &tx,
            content,
            preview_content,
            content_hash,
            clip_type,
            source_app,
            was_trimmed,
            has_leading_whitespace,
            is_multiline,
            size_in_bytes,
            is_file_uri,
        )?;
        if inserted {
            cliptoo_core::stats::increment_stat(&tx, cliptoo_core::stats::KEY_UNIQUE_CLIPS_EVER)?;
        }
        tx.commit()?;
        Ok(inserted)
    })
    .await
}
