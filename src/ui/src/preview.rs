use std::sync::Arc;

use slint::ComponentHandle;
use slint::Model;

use cliptoo_core::image::PREVIEW_FALLBACK_DIM;

use crate::helpers;

pub fn setup_preview(
    ui: &crate::AppWindow,
    db: &Arc<cliptoo_core::db::DbPool>,
    dirs: &crate::app_dirs::AppDirs,
) {
    let preview_db = db.clone();
    let preview_ui = ui.as_weak();
    let preview_fd = dirs.favicons_dir.clone();
    let preview_td = dirs.thumbnails_dir.clone();
    ui.on_request_preview(move |id: i32, x: f32, y: f32| {
        let db = preview_db.clone();
        let ui = preview_ui.clone();
        let fav_dir = preview_fd.clone();
        let td = preview_td.clone();
        tokio::spawn(async move {
            let result = db
                .with(|conn| cliptoo_core::db::queries::get_clip_type_and_content(conn, id as i64))
                .await;
            if let Ok((content, clip_type, content_hash)) = result {
                let _ = ui.upgrade_in_event_loop(move |ui| {
                    let resolved_type: String = match clip_type.as_str() {
                        "code_snippet" => "code_snippet".to_string(),
                        "link" => "link".to_string(),
                        "folder" => "folder".to_string(),
                        _ => {
                            let path = std::path::Path::new(&content);
                            let is_image_file = path.is_file() && {
                                const IMAGE_EXTS: &[&str] = &[
                                    "png", "jpg", "jpeg", "gif", "webp", "svg", "avif", "heic",
                                    "jxl", "ico", "bmp", "tiff", "tif",
                                ];
                                path.extension()
                                    .and_then(|e| e.to_str())
                                    .map(|e| IMAGE_EXTS.contains(&e.to_lowercase().as_str()))
                                    .unwrap_or(false)
                            };
                            if is_image_file {
                                "file_image".to_string()
                            } else if cliptoo_core::color::ColorParser::is_color(&content) {
                                "color".to_string()
                            } else {
                                "text".to_string()
                            }
                        }
                    };

                    let popup_w: f32 = match resolved_type.as_str() {
                        "code_snippet" => 560.0,
                        _ => 400.0,
                    };

                    let window_w = ui.window().size().width as f32;
                    let scale = ui.window().scale_factor();
                    let window_w_logical = window_w / scale;
                    let margin = 8.0_f32;
                    let max_x = (window_w_logical - popup_w - margin).max(margin);
                    let popup_x = (x + 20.0_f32).clamp(margin, max_x);
                    let popup_y = y;
                    ui.set_preview_popup_x(popup_x);
                    ui.set_preview_popup_y(popup_y);

                    match clip_type.as_str() {
                        "code_snippet" => {
                            ui.set_preview_clip_type("code_snippet".into());
                            ui.set_preview_text(content.as_str().into());
                        }
                        "link" => {
                            ui.set_preview_clip_type("link".into());
                            ui.set_preview_text(content.as_str().into());
                            ui.set_preview_favicon(slint::Image::default());
                            ui.set_preview_web_title("".into());
                            let c = content.to_string();
                            let fd = fav_dir.clone();
                            let w = ui.as_weak();
                            let clip_id = id;
                            if let Some(t) = crate::favicon::load_cached_page_title(&c, &fd) {
                                ui.set_preview_web_title(t.into());
                            }
                            tokio::spawn(async move {
                                let cached_title = crate::favicon::load_cached_page_title(&c, &fd);
                                let (title, fav_path) = if cached_title.is_some() {
                                    (None, crate::favicon::fetch_favicon(&c, &fd).await)
                                } else {
                                    let (t, f) = tokio::join!(
                                        helpers::fetch_page_title(&c),
                                        crate::favicon::fetch_favicon(&c, &fd),
                                    );
                                    if let Some(ref t) = t {
                                        crate::favicon::cache_page_title(&c, t, &fd);
                                    }
                                    (t, f)
                                };
                                let _ = w.upgrade_in_event_loop(move |ui| {
                                    if let Some(t) = title.or(cached_title) {
                                        ui.set_preview_web_title(t.into());
                                    }
                                    if let Some(p) = fav_path {
                                        let img =
                                            slint::Image::load_from_path(&p).unwrap_or_default();
                                        ui.set_preview_favicon(img.clone());
                                        let model = ui.get_clips();
                                        for i in 0..model.row_count() {
                                            if let Some(mut data) = model.row_data(i)
                                                && data.id == clip_id
                                            {
                                                data.favicon_image = img;
                                                model.set_row_data(i, data);
                                                break;
                                            }
                                        }
                                    }
                                });
                            });
                        }
                        _ => {
                            if resolved_type == "file_image" {
                                let thumb_path =
                                    td.join(format!("{}_preview.webp", &content_hash[..16]));
                                if thumb_path.exists() {
                                    let img = slint::Image::load_from_path(&thumb_path)
                                        .unwrap_or_default();
                                    ui.set_preview_image(img);
                                } else {
                                    let file_path = content.clone();
                                    let td2 = td.clone();
                                    let hash2 = content_hash.clone();
                                    let w = ui.as_weak();
                                    tokio::spawn(async move {
                                        if let Ok(data) = std::fs::read(&file_path)
                                            && cliptoo_core::image::store_both_thumbnails(
                                                &td2,
                                                &hash2,
                                                &data,
                                                PREVIEW_FALLBACK_DIM,
                                            )
                                            .is_ok()
                                        {
                                            let p =
                                                td2.join(format!("{}_preview.webp", &hash2[..16]));
                                            let _ = w.upgrade_in_event_loop(move |ui| {
                                                let img = slint::Image::load_from_path(&p)
                                                    .unwrap_or_default();
                                                ui.set_preview_image(img);
                                            });
                                        }
                                    });
                                }
                            } else if resolved_type == "folder" {
                                let path = std::path::Path::new(&content);
                                let info = if path.is_dir() {
                                    let mut count = 0u64;
                                    let mut total_size = 0u64;
                                    let mut latest_mtime = 0i64;
                                    if let Ok(entries) = std::fs::read_dir(path) {
                                        for entry in entries.flatten() {
                                            count += 1;
                                            if let Ok(meta) = entry.metadata() {
                                                total_size += meta.len();
                                                if let Ok(mtime) = meta.modified()
                                                    && let Ok(dur) =
                                                        mtime.duration_since(std::time::UNIX_EPOCH)
                                                {
                                                    latest_mtime =
                                                        latest_mtime.max(dur.as_secs() as i64);
                                                }
                                            }
                                        }
                                    }
                                    let size_str = if total_size < 1024 {
                                        format!("{total_size} B")
                                    } else if total_size < 1024 * 1024 {
                                        format!("{:.1} KB", total_size as f64 / 1024.0)
                                    } else if total_size < 1024 * 1024 * 1024 {
                                        format!("{:.1} MB", total_size as f64 / (1024.0 * 1024.0))
                                    } else {
                                        format!(
                                            "{:.2} GB",
                                            total_size as f64 / (1024.0 * 1024.0 * 1024.0)
                                        )
                                    };
                                    let item_label = if count == 1 { "item" } else { "items" };
                                    let date_str = if latest_mtime > 0 {
                                        let secs = latest_mtime;
                                        let days = secs / 86400;
                                        let time_secs = secs % 86400;
                                        let hours = time_secs / 3600;
                                        let minutes = (time_secs % 3600) / 60;
                                        let z = days + 719468;
                                        let era = if z >= 0 { z } else { z - 146096 } / 146097;
                                        let doe = z - era * 146097;
                                        let yoe =
                                            (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
                                        let y = yoe + era * 400;
                                        let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
                                        let mp = (5 * doy + 2) / 153;
                                        let d = doy - (153 * mp + 2) / 5 + 1;
                                        let m = if mp < 10 { mp + 3 } else { mp - 9 };
                                        format!(
                                            "{:04}-{:02}-{:02} {:02}:{:02}",
                                            y, m, d, hours, minutes
                                        )
                                    } else {
                                        String::new()
                                    };
                                    format!("{count} {item_label} · {size_str} · {date_str}")
                                } else {
                                    String::new()
                                };
                                ui.set_preview_file_info(info.into());
                            }
                            ui.set_preview_clip_type(resolved_type.into());
                            ui.set_preview_text(content.into());
                        }
                    }
                    ui.set_preview_visible(true);
                });
            }
        });
    });
}

pub fn setup_dismiss_preview(ui: &crate::AppWindow) {
    let dismiss_ui = ui.as_weak();
    ui.on_dismiss_preview(move || {
        if let Some(ui) = dismiss_ui.upgrade() {
            ui.set_preview_visible(false);
        }
    });
}
