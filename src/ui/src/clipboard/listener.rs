use anyhow::Result;
use cliptoo_core::content::classifier::ContentProcessor;
use cliptoo_core::db::DbPool;
use cliptoo_core::db::models::ClipType;
use cliptoo_core::db::queries::insert_or_bump;
use cliptoo_core::image::HASH_FILENAME_PREFIX_LEN;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::mpsc;
use tracing::{debug, info};
use wl_clipboard_rs::paste::{ClipboardType, Error as WlError, Seat, get_mime_types_ordered};

use crate::paste::PasteSuppressionSet;
use crate::source_app::ActiveWindowCache;

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
    preview_max_dim: Arc<std::sync::atomic::AtomicU32>,
    active_filter_state: Arc<std::sync::Mutex<String>>,
    mut selection_rx: mpsc::Receiver<Option<String>>,
    active_window: ActiveWindowCache,
) -> Result<()> {
    let mut last_text_hash: Option<String> = None;
    let mut last_rtf_hash: Option<String> = None;
    let mut last_html_hash: Option<String> = None;
    let mut last_image_hash: Option<String> = None;
    let mut last_file_hash: Option<String> = None;
    let mut last_mime_types: Option<Vec<String>> = None;
    let mut last_full_read: Option<Instant> = None;
    let mut last_image_probe: Option<Instant> = None;
    // True while the current clipboard generation (mime set) has already been
    // ingested through a non-text channel (file/uri-list, image, RTF, HTML).
    // The text/plain rendition of that same selection is accessory and must
    // not spawn a separate Text clip; reset on mime change so a fresh
    // plain-text copy is ingested normally.
    let mut non_text_ingested = false;
    const POLL_INTERVAL: Duration = Duration::from_millis(500);
    const FULL_READ_INTERVAL: Duration = Duration::from_secs(5);
    // Image payloads must be downloaded in full to recompute their hash, so a
    // static image clipboard costs real bandwidth on every probe. Re-probe an
    // *unchanged* image at this slow cadence instead of on every cheap text
    // stale-read; a freshly ingested image keeps last_image_probe cleared so a
    // rapid re-copy (image A then B, same mime set) is still caught quickly.
    const IMAGE_RECHECK_INTERVAL: Duration = Duration::from_secs(30);

    // True until the clipboard's state at startup has been observed. Content
    // already present when Cliptoo starts (left over from before launch) must
    // not be ingested as a "new" clip; only changes after startup count.
    let mut baseline = true;

    // Source-app snapshot for the clipboard event being ingested: set from the
    // data-control watcher, which captures the active window at the instant of
    // the copy — before any focus switch can land. `None` here means no
    // watcher event is pending and the tracker cache is used as a fallback.
    let mut pending_source_app: Option<Option<String>> = None;

    loop {
        // Prefer a queued selection event over mime detection: it carries the
        // source-app snapshot taken at the copy moment and fires even for a
        // re-copy whose mime set is unchanged. Draining keeps the last event's
        // snapshot, which matches the (latest) clipboard content about to be
        // read.
        while let Ok(app) = selection_rx.try_recv() {
            pending_source_app = Some(app);
            last_mime_types = None;
        }

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
                last_html_hash = None;
                last_image_hash = None;
                last_file_hash = None;
                last_mime_types = None;
                last_full_read = None;
                last_image_probe = None;
                non_text_ingested = false;
                pending_source_app = None;
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
        let image_recheck_due =
            last_image_probe.is_none_or(|t| t.elapsed() >= IMAGE_RECHECK_INTERVAL);
        let probe_images = changed || image_recheck_due;

        if changed && !baseline {
            // New copy: a previously ingested non-text clip no longer covers
            // the current selection, so its accessory text/plain must not be
            // suppressed anymore.
            non_text_ingested = false;
            // A fresh generation may offer the same plain text that was read as
            // the accessory rendition of the previous one; drop its seed so the
            // genuine copy is ingested instead of matching that rendition.
            // Gated on `!baseline`: the forced baseline re-reads re-observe the
            // same startup content and must keep their seeds (see below).
            last_text_hash = None;
        }

        if !changed && !stale {
            // Wait for a selection change (data-control watcher) or the poll
            // interval, whichever comes first. The watcher snapshots the
            // source app at the exact moment of the copy, so a focus switch
            // that lands before ingest can't be mistaken for the copy's
            // source. If the watcher is gone (no data-control on the
            // compositor) the timeout keeps the plain polling cadence.
            match tokio::time::timeout(POLL_INTERVAL, selection_rx.recv()).await {
                Ok(Some(app)) => {
                    pending_source_app = Some(app);
                    // Force a poll even when the mime set is unchanged (a
                    // re-copy of the same kind of content offers the same
                    // mimes); `changed` is recomputed next iteration.
                    last_mime_types = None;
                }
                Ok(None) => {
                    tokio::time::sleep(POLL_INTERVAL).await;
                }
                Err(_) => {}
            }
            continue;
        }

        // The mime list is passed to the reader so it can probe image/* before
        // text/plain: a browser image copy offers image/* AND a non-empty
        // text/plain (URL/alt text), and reading text first would record a
        // spurious Link/Text clip while deferring the image to a stale read.
        // `probe_images` gates the image download (see IMAGE_RECHECK_INTERVAL).
        let result = poll_clipboard(
            &mut last_text_hash,
            &mut last_rtf_hash,
            &mut last_html_hash,
            &mut last_image_hash,
            &mut last_file_hash,
            mime_types.as_deref(),
            probe_images,
        )
        .await;

        // A probe that found nothing new (image unchanged/absent) starts the
        // slow recheck window; a probe that yielded a fresh image leaves the
        // window cleared so a rapid re-copy is still caught on the next read.
        let image_found = matches!(
            result.as_ref().ok().and_then(Option::as_ref),
            Some(ClipboardPayload::Image { .. })
        );
        if probe_images && !image_found {
            last_image_probe = Some(Instant::now());
        }

        last_mime_types = mime_types;
        last_full_read = Some(Instant::now());

        match result {
            Ok(Some(payload)) => {
                if baseline {
                    // Startup: the pre-existing selection must not be
                    // ingested. One read stops at the first rendition it can
                    // classify (image, rich markup, or plain text), so keep
                    // forcing reads — with the seeds kept, see the gated reset
                    // above — until a read yields nothing new and every
                    // rendition's hash is seeded; a later full read then
                    // dedups instead of ingesting an accessory rendition as a
                    // new clip. Drop any watcher snapshot from startup so it
                    // can't be attributed to the first genuine copy.
                    pending_source_app = None;
                    last_mime_types = None;
                    debug!("clipboard: baseline read; seeding change detection");
                    continue;
                }

                // The source app: prefer the watcher's snapshot taken at the
                // moment of the copy; fall back to the tracker cache for
                // stale/poll-detected reads. Taken unconditionally so a
                // snapshot never leaks into a later, unrelated poll.
                let source_app = match pending_source_app.take() {
                    Some(app) => app,
                    None => crate::source_app::current_active_app(&active_window),
                };
                debug!("clipboard: source app {source_app:?}");

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
                            // The text/plain rendition of a clipboard whose
                            // non-text content (file/uri-list, image, RTF, HTML)
                            // was already ingested this generation is accessory
                            // and must not spawn a separate Text clip. Gated on
                            // ingestion rather than advertised mimes: a clipboard
                            // that advertises a rich type with an *empty* payload
                            // (some producers) has no non-text clip yet, so its
                            // plain text is the genuine content and must be kept.
                            if non_text_ingested {
                                debug!(
                                    "clipboard: accessory plain text on a non-text clipboard skipped"
                                );
                                // The hash stays seeded: an unchanged non-text
                                // clipboard re-reads (and re-logs) its accessory
                                // text/plain on every stale poll otherwise. The
                                // seed is dropped on mime change instead, where a
                                // genuine re-copy becomes distinguishable.
                                continue;
                            }

                            if is_blacklisted_live(&blacklist_state, source_app.as_deref()) {
                                debug!("blacklisted app {source_app:?} — skipping text clip");
                                continue;
                            }

                            // A rich (Rtf/Html) clip is a non-text content type;
                            // its accessory text/plain rendition must be
                            // suppressed for the rest of this generation.
                            if classified.clip_type == ClipType::Rtf
                                || classified.clip_type == ClipType::Html
                            {
                                non_text_ingested = true;
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
                        if is_blacklisted_live(&blacklist_state, source_app.as_deref()) {
                            debug!("blacklisted app {source_app:?} — skipping file-uri clip");
                            continue;
                        }

                        // A file/folder clip is non-text content; the path it
                        // also offers as text/plain is an accessory rendition
                        // that must not become a separate `file_path` clip.
                        non_text_ingested = true;

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

                        let filter = active_filter_state.lock().unwrap().clone();
                        if inserted {
                            let clip_type = c.clip_type.as_str();
                            info!("new file-uri clip: {} — {clip_type}", &c.content_hash[..12]);
                            if clip_type == "file_image" {
                                // Decode + thumbnail writes are blocking and slow
                                // for large image files; hand the store-and-refresh
                                // off to a task so the poll loop keeps detecting
                                // new copies. The inline refresh is skipped so the
                                // list isn't rebuilt with an empty (and then
                                // cached-empty) thumbnail before the files land.
                                let path = std::path::Path::new(&c.content).to_owned();
                                let hash_c = c.content_hash.clone();
                                let td = thumbnails_dir.clone();
                                let db2 = db.clone();
                                let ui2 = ui.clone();
                                let td2 = thumbnails_dir.clone();
                                let fd2 = favicons_dir.clone();
                                let max_dim = preview_max_dim.clone();
                                std::mem::drop(tokio::spawn(async move {
                                    let store = tokio::task::spawn_blocking(move || {
                                        if let Err(e) =
                                            cliptoo_core::image::store_both_thumbnails_for_file(
                                                &td,
                                                &hash_c,
                                                &path,
                                                max_dim.load(std::sync::atomic::Ordering::Relaxed),
                                            )
                                        {
                                            tracing::error!("store_both_thumbnails_for_file: {e}");
                                        }
                                    });
                                    let _ = store.await;
                                    refresh_clips(&db2, &ui2, &td2, &fd2, "", &filter, None).await;
                                }));
                            } else {
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
                        } else {
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
                    ClipboardPayload::Image {
                        hash, data, mime, ..
                    } => {
                        if is_blacklisted_live(&blacklist_state, source_app.as_deref()) {
                            debug!("blacklisted app {source_app:?} — skipping image clip");
                            continue;
                        }

                        // An image clip is non-text content; the image URL/alt
                        // text it also offers as text/plain is an accessory
                        // rendition that must not spawn a spurious Link/Text
                        // clip for the rest of this generation.
                        non_text_ingested = true;

                        let content_str = images_dir
                            .join(format!("{}.png", &hash[..HASH_FILENAME_PREFIX_LEN]))
                            .to_string_lossy()
                            .to_string();
                        // A real description (mime + size) so the list row and
                        // export carry meaningful text instead of the phantom
                        // "clipboard-image-{hash}.png" placeholder that matched
                        // no file on disk.
                        let size = data.len() as i64;
                        let preview = format!("{mime} · {size} bytes");

                        let images = images_dir.clone();
                        let thumbnails = thumbnails_dir.clone();
                        let db2 = db.clone();
                        let ui2 = ui.clone();
                        let td2 = thumbnails_dir.clone();
                        let fd2 = favicons_dir.clone();
                        let filter2 = active_filter_state.lock().unwrap().clone();
                        let max_dim = preview_max_dim.clone();
                        // The full-res PNG is what the row's Content references,
                        // so it must exist on disk before the row is recorded; a
                        // write that fails (corrupt/undecodable input, disk full)
                        // would otherwise leave an orphan clip pointing at
                        // nothing. Decode + writes are also slow for large
                        // images, so the whole store-then-insert runs off the
                        // poll loop to keep detecting new copies.
                        std::mem::drop(tokio::spawn(async move {
                            let store_hash = hash.clone();
                            let store = tokio::task::spawn_blocking(move || -> Result<()> {
                                cliptoo_core::image::store_image(&images, &store_hash, &data)?;
                                // Thumbnail failure is non-fatal: the row still
                                // references a real PNG, and thumbnails are
                                // regenerated on demand (preview.rs).
                                if let Err(e) = cliptoo_core::image::store_both_thumbnails(
                                    &thumbnails,
                                    &store_hash,
                                    &data,
                                    max_dim.load(std::sync::atomic::Ordering::Relaxed),
                                ) {
                                    tracing::warn!("store_both_thumbnails: {e}");
                                }
                                Ok(())
                            });
                            let stored_ok = match store.await {
                                Ok(Ok(())) => true,
                                Ok(Err(e)) => {
                                    tracing::error!("image store failed; clip not recorded: {e}");
                                    false
                                }
                                Err(e) => {
                                    tracing::error!("image store task panicked: {e}");
                                    false
                                }
                            };
                            if !stored_ok {
                                return;
                            }
                            let inserted = match insert_clip_with_stat(
                                &db2,
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
                                    return;
                                }
                            };
                            if inserted {
                                info!("new image clip: {} ({} bytes)", &hash[..12], size);
                            } else {
                                info!("existing image clip updated: {}", &hash[..12]);
                            }
                            refresh_clips(&db2, &ui2, &td2, &fd2, "", &filter2, None).await;
                        }));
                    }
                }
            }
            Ok(None) => {
                // A readable but content-free clipboard (or no change) also
                // means the startup state has been observed.
                pending_source_app = None;
                baseline = false;
            }
            Err(e) => {
                pending_source_app = None;
                tracing::error!("Clipboard poll error: {e}");
            }
        }
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
