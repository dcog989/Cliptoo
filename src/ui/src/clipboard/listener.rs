use anyhow::Result;
use cliptoo_core::content::classifier::ContentProcessor;
use cliptoo_core::db::DbPool;
use cliptoo_core::db::queries::insert_or_bump;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tracing::{debug, info};
use wl_clipboard_rs::paste::{ClipboardType, Error as WlError, Seat, get_mime_types_ordered};

use crate::paste::PasteSuppressionSet;

use super::ClipboardPayload;
use super::is_blacklisted;
use super::reader::poll_clipboard;
use super::refresh::refresh_ui;

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
) -> Result<()> {
    let mut last_text_hash: Option<String> = None;
    let mut last_image_hash: Option<String> = None;
    let mut last_file_hash: Option<String> = None;
    let mut last_mime_types: Option<Vec<String>> = None;
    let mut last_full_read: Option<Instant> = None;
    const POLL_INTERVAL: Duration = Duration::from_millis(500);
    const FULL_READ_INTERVAL: Duration = Duration::from_secs(5);

    loop {
        let mime_types = match tokio::task::spawn_blocking(|| {
            get_mime_types_ordered(ClipboardType::Regular, Seat::Unspecified)
        })
        .await
        {
            Ok(Ok(mt)) => Some(mt),
            Ok(Err(WlError::ClipboardEmpty | WlError::NoSeats)) => {
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
                let sup_hash = match &payload {
                    ClipboardPayload::Text { content, .. }
                    | ClipboardPayload::FileUri { content, .. } => {
                        let normalized =
                            cliptoo_core::content::hash::normalize_line_endings(content);
                        cliptoo_core::content::hash::sha256_u64(&normalized)
                    }
                    ClipboardPayload::Image { .. } => 0,
                };

                if sup_hash != 0 && suppression.check_and_remove(sup_hash) {
                    info!("suppressed re-ingest of own paste");
                    continue;
                }

                match payload {
                    ClipboardPayload::Text { hash, content } => {
                        let classified = ContentProcessor::process(&content);
                        if classified.is_none() {
                            debug!("clipboard: empty/whitespace-only text skipped");
                        }
                        if let Some(classified) = classified {
                            if classified.clip_type.is_file_type() {
                                let file_hash =
                                    cliptoo_core::content::hash::sha256_hex(&classified.content);
                                last_file_hash = Some(file_hash);
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
                                        &hash,
                                        classified.clip_type.as_str(),
                                        source_app.as_deref(),
                                        classified.was_trimmed,
                                        classified.has_leading_whitespace,
                                        classified.is_multiline,
                                        classified.size_in_bytes,
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
                                let thumb_handle = if classified.clip_type
                                    == cliptoo_core::db::models::ClipType::FileImage
                                {
                                    let path = std::path::PathBuf::from(&classified.content);
                                    let hash_c = hash.clone();
                                    let td = thumbnails_dir.clone();
                                    Some(tokio::task::spawn_blocking(move || {
                                        if let Ok(data) = std::fs::read(&path) {
                                            if let Err(e) =
                                                cliptoo_core::image::store_both_thumbnails(
                                                    &td,
                                                    &hash_c,
                                                    &data,
                                                    preview_max_dim,
                                                )
                                            {
                                                tracing::error!("store_both_thumbnails: {e}");
                                            }
                                        } else {
                                            tracing::error!(
                                                "read file for thumbnail: {} — {path:?}",
                                                &hash_c[..12]
                                            );
                                        }
                                    }))
                                } else {
                                    None
                                };

                                let sa = source_app.as_deref().unwrap_or("unknown");
                                info!(
                                    "new clip: {} — {:?} (from {sa})",
                                    &hash[..12],
                                    classified.clip_type
                                );
                                if let Some(h) = thumb_handle {
                                    let _ = h.await;
                                }
                            } else {
                                info!("existing clip updated: {} — text", &hash[..12]);
                            }

                            refresh_ui(&db, &ui, &thumbnails_dir, &favicons_dir).await;
                        }
                    }
                    ClipboardPayload::FileUri { hash, content } => {
                        let source_app = crate::source_app::detect_source_app().await;

                        if is_blacklisted(source_app.as_deref(), &blacklisted_apps) {
                            debug!("blacklisted app {source_app:?} — skipping file-uri clip");
                            continue;
                        }

                        let inserted = {
                            let classified = ContentProcessor::process(&content);
                            let (clip_type, preview_content, size, is_multiline) =
                                if let Some(ref c) = classified {
                                    (
                                        c.clip_type.as_str().to_string(),
                                        c.preview_content.clone(),
                                        c.size_in_bytes,
                                        c.is_multiline,
                                    )
                                } else {
                                    (
                                        "file_generic".to_string(),
                                        content[..content.len().min(200)].to_string(),
                                        content.len() as i64,
                                        content.contains('\n'),
                                    )
                                };
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
                                    if let Ok(data) = std::fs::read(&path) {
                                        if let Err(e) = cliptoo_core::image::store_both_thumbnails(
                                            &td,
                                            &hash_c,
                                            &data,
                                            preview_max_dim,
                                        ) {
                                            tracing::error!("store_both_thumbnails: {e}");
                                        }
                                    } else {
                                        tracing::error!(
                                            "read file for thumbnail: {} — {path:?}",
                                            &hash_c[..12]
                                        );
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
                        refresh_ui(&db, &ui, &thumbnails_dir, &favicons_dir).await;
                    }
                    ClipboardPayload::Image { hash, data } => {
                        let source_app = crate::source_app::detect_source_app().await;

                        if is_blacklisted(source_app.as_deref(), &blacklisted_apps) {
                            debug!("blacklisted app {source_app:?} — skipping image clip");
                            continue;
                        }

                        let content_str = images_dir
                            .join(format!("{}.png", &hash[..16]))
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

                        refresh_ui(&db, &ui, &thumbnails_dir, &favicons_dir).await;
                    }
                }
            }
            Ok(None) => {}
            Err(e) => tracing::error!("Clipboard poll error: {e}"),
        }

        tokio::time::sleep(POLL_INTERVAL).await;
    }
}
