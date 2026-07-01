use std::sync::Arc;

use slint::ComponentHandle;
use slint::Model;

use crate::helpers;

pub fn setup_clip_actions(
    ui: &crate::AppWindow,
    edit_win: &crate::EditWindow,
    db: &Arc<cliptoo_core::db::DbPool>,
    settings: &std::rc::Rc<std::cell::RefCell<cliptoo_core::Settings>>,
    dirs: &crate::app_dirs::AppDirs,
    suppression: &Arc<crate::paste::PasteSuppressionSet>,
) {
    // ── Context menu: send-to app names ─────────────────────────────────
    {
        let names: Vec<slint::SharedString> = settings
            .borrow()
            .send_to_apps
            .iter()
            .map(|a| slint::SharedString::from(a.name.as_str()))
            .collect();
        let model = std::rc::Rc::new(slint::VecModel::<slint::SharedString>::from(names));
        ui.set_ctx_send_to_apps(model.into());
    }

    let thumbnails_dir = dirs.thumbnails_dir.clone();
    let favicons_dir = dirs.favicons_dir.clone();

    // ── Delete clip ──────────────────────────────────────────────────────
    {
        let del_db = db.clone();
        let del_ui = ui.as_weak();
        let del_td = thumbnails_dir.clone();
        let del_fd = favicons_dir.clone();
        ui.on_delete_clip(move |id: i32| {
            let db = del_db.clone();
            let ui = del_ui.clone();
            let td = del_td.clone();
            let fd = del_fd.clone();
            tokio::spawn(async move {
                let _ = db
                    .with(|conn| cliptoo_core::db::queries::delete_clip(conn, id as i64))
                    .await;
                helpers::refresh_clips(&db, &ui, &td, &fd, "", "", None).await;
            });
        });
    }

    // ── Toggle bookmark ──────────────────────────────────────────────────
    {
        let bm_db = db.clone();
        let bm_ui = ui.as_weak();
        ui.on_toggle_bookmark(move |id: i32, current: bool| {
            let db = bm_db.clone();
            let ui = bm_ui.clone();
            tokio::spawn(async move {
                let _ = db
                    .with(|conn| {
                        cliptoo_core::db::queries::set_bookmarked(conn, id as i64, !current)
                    })
                    .await;
                let _ = ui.upgrade_in_event_loop(move |ui| {
                    let model = ui.get_clips();
                    for i in 0..model.row_count() {
                        if let Some(mut data) = model.row_data(i)
                            && data.id == id
                        {
                            data.is_bookmarked = !current;
                            model.set_row_data(i, data);
                            break;
                        }
                    }
                });
            });
        });
    }

    // ── Move to top ──────────────────────────────────────────────────────
    {
        let mtt_db = db.clone();
        let mtt_ui = ui.as_weak();
        let mtt_td = thumbnails_dir.clone();
        let mtt_fd = favicons_dir.clone();
        ui.on_move_to_top(move |id: i32| {
            let db = mtt_db.clone();
            let ui = mtt_ui.clone();
            let td = mtt_td.clone();
            let fd = mtt_fd.clone();
            tokio::spawn(async move {
                let _ = db
                    .with(|conn| cliptoo_core::db::queries::bump_to_top(conn, id as i64))
                    .await;
                helpers::refresh_clips(&db, &ui, &td, &fd, "", "", None).await;
            });
        });
    }

    // ── Edit clip (from context menu) ───────────────────────────────────
    {
        let ctx_edit_win = edit_win.as_weak();
        let ctx_edit_settings = settings.clone();
        let ctx_edit_db = db.clone();
        let ctx_edit_ui = ui.as_weak();
        ui.on_edit_clip(move |id: i32| {
            let db = ctx_edit_db.clone();
            let edit = ctx_edit_win.clone();
            let ui = ctx_edit_ui.clone();
            let (edit_w, edit_h) = {
                let s = ctx_edit_settings.borrow();
                (s.editor_window_width as f32, s.editor_window_height as f32)
            };
            tokio::spawn(async move {
                let content = db
                    .with(|conn| cliptoo_core::db::queries::get_clip_content(conn, id as i64))
                    .await
                    .unwrap_or_default();
                let clip_type = db
                    .with(|conn| {
                        cliptoo_core::db::queries::get_clip_type_and_content(conn, id as i64)
                    })
                    .await
                    .map(|(_, t, _)| t)
                    .unwrap_or_default();
                let tags = db
                    .with(|conn| cliptoo_core::db::queries::get_clip_tags(conn, id as i64))
                    .await
                    .unwrap_or_default();
                let char_count = content.chars().count();
                let size_str = if content.len() > 1024 {
                    format!("{:.1} KB", content.len() as f64 / 1024.0)
                } else {
                    format!("{} B", content.len())
                };
                let meta = format!("{} · {} chars", size_str, char_count);

                let is_code = clip_type == "code_snippet" || clip_type == "file_dev";

                let _ = edit.upgrade_in_event_loop(move |edit| {
                    edit.set_clip_id(id);
                    edit.set_edit_content(content.into());
                    edit.set_initial_tags(tags.into());
                    edit.set_clip_type(clip_type.into());
                    edit.set_meta_info(meta.into());
                    edit.window().set_size(slint::LogicalSize {
                        width: edit_w,
                        height: edit_h,
                    });
                    if let Some(ui) = ui.upgrade() {
                        crate::positioning::position_editor_relative_to_main(&edit, &ui);
                    }

                    edit.set_editing(!is_code);

                    let _ = edit.show();
                });
            });
        });
    }

    // ── Move to bottom ───────────────────────────────────────────────────
    {
        let mtb_db = db.clone();
        let mtb_ui = ui.as_weak();
        let mtb_td = thumbnails_dir.clone();
        let mtb_fd = favicons_dir.clone();
        ui.on_move_to_bottom(move |id: i32| {
            let db = mtb_db.clone();
            let ui = mtb_ui.clone();
            let td = mtb_td.clone();
            let fd = mtb_fd.clone();
            tokio::spawn(async move {
                let _ = db
                    .with(|conn| cliptoo_core::db::queries::bump_to_bottom(conn, id as i64))
                    .await;
                helpers::refresh_clips(&db, &ui, &td, &fd, "", "", None).await;
            });
        });
    }

    // ── Move up one ──────────────────────────────────────────────────────
    {
        let muo_db = db.clone();
        let muo_ui = ui.as_weak();
        let muo_td = thumbnails_dir.clone();
        let muo_fd = favicons_dir.clone();
        ui.on_move_up_one(move |id: i32| {
            let db = muo_db.clone();
            let ui = muo_ui.clone();
            let td = muo_td.clone();
            let fd = muo_fd.clone();
            tokio::spawn(async move {
                let _ = db
                    .with(|conn| cliptoo_core::db::queries::move_up_one(conn, id as i64))
                    .await;
                helpers::refresh_clips(&db, &ui, &td, &fd, "", "", None).await;
            });
        });
    }

    // ── Move down one ────────────────────────────────────────────────────
    {
        let mdo_db = db.clone();
        let mdo_ui = ui.as_weak();
        let mdo_td = thumbnails_dir.clone();
        let mdo_fd = favicons_dir.clone();
        ui.on_move_down_one(move |id: i32| {
            let db = mdo_db.clone();
            let ui = mdo_ui.clone();
            let td = mdo_td.clone();
            let fd = mdo_fd.clone();
            tokio::spawn(async move {
                let _ = db
                    .with(|conn| cliptoo_core::db::queries::move_down_one(conn, id as i64))
                    .await;
                helpers::refresh_clips(&db, &ui, &td, &fd, "", "", None).await;
            });
        });
    }

    // ── Compare clips ────────────────────────────────────────────────────
    let compare_left_id: std::rc::Rc<std::cell::Cell<Option<i64>>> =
        std::rc::Rc::new(std::cell::Cell::new(None));

    // ── Transform clip ───────────────────────────────────────────────────
    {
        let transform_db = db.clone();
        let transform_ui = ui.as_weak();
        let transform_sup = suppression.clone();
        ui.on_transform_clip(move |id: i32, key: slint::SharedString| {
            let db = transform_db.clone();
            let ui = transform_ui.clone();
            let sup = transform_sup.clone();
            let key = key.to_string();
            tokio::spawn(async move {
                let content = db
                    .with(|conn| cliptoo_core::db::queries::get_clip_content(conn, id as i64))
                    .await;
                if let Ok(content) = content {
                    let transformed = cliptoo_core::transform::transform(&content, &key);
                    if let Err(e) =
                        crate::paste::paste_content(&transformed, "text", &sup, &ui, false).await
                    {
                        tracing::error!("Transform paste error (key={key}): {e}");
                    }
                }
            });
        });
    }

    // ── Send to app ──────────────────────────────────────────────────────
    {
        let sendto_db = db.clone();
        let sendto_settings = settings.clone();
        ui.on_send_to_clip(move |id: i32, app_path: slint::SharedString| {
            let db = sendto_db.clone();
            let app_path = app_path.to_string();
            let resolved_path = {
                let s = sendto_settings.borrow();
                s.send_to_apps
                    .iter()
                    .find(|a| a.name == app_path || a.path == app_path)
                    .map(|a| a.path.clone())
                    .unwrap_or_else(|| app_path.clone())
            };
            tokio::spawn(async move {
                let content = db
                    .with(|conn| cliptoo_core::db::queries::get_clip_content(conn, id as i64))
                    .await;
                if let Ok(content) = content
                    && let Err(e) = cliptoo_core::send_to::send_to(&resolved_path, &content).await
                {
                    tracing::error!("Send-to error (app='{resolved_path}'): {e}");
                }
            });
        });
    }

    // ── Compare clips ────────────────────────────────────────────────────
    {
        let cl = compare_left_id.clone();
        ui.on_compare_set_left(move |id: i32| {
            cl.set(Some(id as i64));
            tracing::debug!("Compare: left set to id={id}");
        });
    }
    {
        let cl = compare_left_id.clone();
        let cmp_db = db.clone();
        let cmp_settings = settings.clone();
        ui.on_compare_right(move |id: i32| {
            let left_id = match cl.get() {
                Some(l) => l,
                None => {
                    tracing::warn!("Compare: no left clip set");
                    return;
                }
            };
            let right_id = id as i64;
            let db = cmp_db.clone();
            let tool_path = cmp_settings.borrow().compare_tool_path.clone();
            tokio::spawn(async move {
                let left = db
                    .with(|conn| cliptoo_core::db::queries::get_clip_content(conn, left_id))
                    .await;
                let right = db
                    .with(|conn| cliptoo_core::db::queries::get_clip_content(conn, right_id))
                    .await;
                match (left, right) {
                    (Ok(l), Ok(r)) => {
                        if let Err(e) =
                            cliptoo_core::compare::compare_clips(&l, &r, &tool_path).await
                        {
                            tracing::error!("Compare error: {e}");
                        }
                    }
                    _ => tracing::error!("Compare: failed to fetch clip content"),
                }
            });
        });
    }

    // ── Item activated (paste on click / Enter) ─────────────────────────
    {
        let paste_db = db.clone();
        let paste_ui = ui.as_weak();
        let paste_sup = suppression.clone();
        let paste_plain = settings.borrow().paste_as_plain_text;
        ui.on_item_activated(move |id: i32| {
            let db = paste_db.clone();
            let ui = paste_ui.clone();
            let sup = paste_sup.clone();
            tokio::spawn(async move {
                let _ = db
                    .with(|conn| cliptoo_core::db::queries::record_paste(conn, id as i64))
                    .await;
                let result = db
                    .with(|conn| {
                        cliptoo_core::db::queries::get_clip_type_and_content(conn, id as i64)
                    })
                    .await;
                if let Ok((content, clip_type, _hash)) = result
                    && let Err(e) =
                        crate::paste::paste_content(&content, &clip_type, &sup, &ui, paste_plain)
                            .await
                {
                    tracing::error!("Paste error: {e}");
                }
            });
        });
    }
}
