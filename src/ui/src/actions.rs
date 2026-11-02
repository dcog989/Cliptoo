use std::sync::Arc;

use slint::ComponentHandle;
use slint::Model;

use crate::helpers;

/// Handles shared by the clip actions, cloned per callback so each background
/// task owns its copies.
#[derive(Clone)]
struct ClipDeps {
    db: Arc<cliptoo_core::db::DbPool>,
    ui: slint::Weak<crate::AppWindow>,
    thumbs: std::path::PathBuf,
    favicons: std::path::PathBuf,
    suppression: Arc<crate::paste::PasteSuppressionSet>,
    tag_prefix: String,
}

impl ClipDeps {
    async fn refresh(&self, view: &helpers::ViewState) {
        helpers::refresh_clips(
            &self.db,
            &self.ui,
            &self.thumbs,
            &self.favicons,
            view,
            Some(&self.tag_prefix),
        )
        .await;
    }
}

pub fn setup_clip_actions(
    ui: &crate::AppWindow,
    edit_win: &crate::EditWindow,
    db: &Arc<cliptoo_core::db::DbPool>,
    settings: &std::rc::Rc<std::cell::RefCell<cliptoo_core::Settings>>,
    dirs: &crate::app_dirs::AppDirs,
    suppression: &Arc<crate::paste::PasteSuppressionSet>,
    tag_prefix: &str,
) {
    let tag_prefix = tag_prefix.to_string();

    // Shared compare state: the context menu's "Set as left" writes here and
    // "Compare" consumes it. Declared at the top so the delete handler can
    // also clear it when the selected left clip is deleted.
    let compare_left_id: std::rc::Rc<std::cell::Cell<Option<i64>>> =
        std::rc::Rc::new(std::cell::Cell::new(None));

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

    let deps = ClipDeps {
        db: db.clone(),
        ui: ui.as_weak(),
        thumbs: dirs.thumbnails_dir.clone(),
        favicons: dirs.favicons_dir.clone(),
        suppression: suppression.clone(),
        tag_prefix: tag_prefix.clone(),
    };

    // ── Delete clip ──────────────────────────────────────────────────────
    {
        let del_deps = deps.clone();
        let del_cmp = compare_left_id.clone();
        ui.on_delete_clip(move |id: i32| {
            // A deleted clip can no longer be the compare-left; drop a stale
            // reference so a later compare doesn't target a missing row.
            if del_cmp.get() == Some(id as i64) {
                del_cmp.set(None);
            }
            let deps = del_deps.clone();
            let view = helpers::current_view_state(&deps.ui);
            // The row can be removed from the model in place when the model
            // holds every row of the view; otherwise a re-query is needed to
            // refill the truncated list. See helpers::model_is_complete.
            let in_place = deps
                .ui
                .upgrade()
                .is_some_and(|ui| helpers::model_is_complete(&ui));
            tokio::spawn(async move {
                let db_ok = deps
                    .db
                    .with(|conn| cliptoo_core::db::queries::delete_clip(conn, id as i64))
                    .await
                    .is_ok();
                if in_place && db_ok {
                    let deps2 = deps.clone();
                    let _ = deps.ui.upgrade_in_event_loop(move |ui| {
                        if !helpers::remove_rows_by_id(&ui, &[id]) {
                            // The model changed since the click; re-query so the
                            // deleted row cannot linger.
                            helpers::refresh_clips_live(
                                &deps2.db,
                                &deps2.ui,
                                &deps2.thumbs,
                                &deps2.favicons,
                                Some(&deps2.tag_prefix),
                            );
                        }
                    });
                } else {
                    deps.refresh(&view).await;
                }
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
                // Only reflect a write that actually happened: flipping the row
                // on a busy/full DB leaves the icon lying until the next refresh.
                if let Err(e) = db
                    .with(|conn| {
                        cliptoo_core::db::queries::set_bookmarked(conn, id as i64, !current)
                    })
                    .await
                {
                    tracing::error!("bookmark toggle failed for clip {id}: {e}");
                    let _ = ui.upgrade_in_event_loop(|ui| {
                        show_main_toast(&ui, "Could not save bookmark", "error");
                    });
                    return;
                }
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
        let mtt_deps = deps.clone();
        let mtt_settings = settings.clone();
        ui.on_move_to_top(move |id: i32| {
            let deps = mtt_deps.clone();
            // Same rich/plain choice a copy or paste would make.
            let offer_rich = !mtt_settings.borrow().paste_as_plain_text;
            let view = helpers::current_view_state(&deps.ui);
            move_clip(deps, view, id, helpers::RowMove::Top, Some(offer_rich));
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
            // Snapshot for re-applying the theme when the editor opens, so a
            // theme change made since startup is reflected here too.
            let s_snap = ctx_edit_settings.borrow().clone();
            tokio::spawn(async move {
                let (mut content, clip_type) = db
                    .with(|conn| {
                        cliptoo_core::db::queries::get_clip_type_and_content(conn, id as i64)
                    })
                    .await
                    .map(|(c, t, _)| (c, t))
                    .unwrap_or_default();
                // A file clip stores the file path, not its text. Load the file's
                // actual contents so the editor — and its read-only code view —
                // opens on the file rather than on a bare path, whatever the
                // extension. A missing, binary or oversized file keeps the path
                // text. The mtime/size snapshot lets Save refuse to overwrite a
                // file that changed on disk after this read.
                let mut file_snapshot: Option<(f64, u64)> = None;
                if cliptoo_core::db::models::ClipType::parse(&clip_type).is_text_file() {
                    let path = content.clone();
                    let read = tokio::task::spawn_blocking(move || load_text_file(&path)).await;
                    if let Ok(Some((text, mtime, size))) = read {
                        content = text;
                        file_snapshot = Some((mtime, size));
                    }
                }
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
                    match file_snapshot {
                        Some((mtime, size)) => crate::edit::remember_file_snapshot(id, mtime, size),
                        None => crate::edit::forget_file_snapshot(),
                    }
                    // Fresh open: clear any dirty/discard state left by the
                    // previous clip (see EditWindow.slint dirty).
                    edit.set_dirty(false);
                    edit.set_confirm_cancel(false);
                    edit.window().set_size(slint::LogicalSize {
                        width: edit_w,
                        height: edit_h,
                    });
                    if let Some(ui) = ui.upgrade() {
                        // Guard blur-to-tray while the editor is visible; cleared
                        // when the editor closes (see edit.rs).
                        ui.set_edit_open(true);
                    }

                    edit.set_editing(!is_code);

                    // Slint globals are per-window-instance: the editor has its
                    // own `Theme` global that must be (re-)filled before showing.
                    let (is_dark, system_accent) = crate::theme::cached_resolved_theme();
                    crate::theme::fill_theme(
                        &edit.global::<crate::Theme>(),
                        &s_snap,
                        is_dark,
                        system_accent,
                    );

                    let _ = edit.show();
                });
            });
        });
    }

    // ── Copy clip to the clipboard without pasting (Ctrl+C) ─────────────
    {
        let copy_deps = deps.clone();
        let copy_settings = settings.clone();
        ui.on_copy_clip(move |id: i32| {
            let deps = copy_deps.clone();
            // Same rich/plain choice a paste would make, read per activation so
            // a settings change applies without a restart.
            let offer_rich = !copy_settings.borrow().paste_as_plain_text;
            tokio::spawn(async move {
                match deps
                    .db
                    .with(|conn| {
                        cliptoo_core::db::queries::get_clip_type_and_content(conn, id as i64)
                    })
                    .await
                {
                    Ok((content, clip_type, _hash)) => {
                        // Keep the clipboard listener from re-ingesting our write.
                        crate::paste::register_suppression(&deps.suppression, &content, &clip_type);
                        if let Err(e) = crate::paste::write_content_to_clipboard(
                            &content, &clip_type, offer_rich,
                        )
                        .await
                        {
                            tracing::error!("Copy error: {e}");
                            let ui = deps.ui.clone();
                            let message = format!("Copy failed: {e:#}");
                            let _ = ui.upgrade_in_event_loop(move |ui| {
                                show_main_toast(&ui, &message, "error");
                            });
                        }
                    }
                    Err(e) => report_missing_clip(&deps, id, &e),
                }
            });
        });
    }

    // ── Reorder ──────────────────────────────────────────────────────────
    {
        let mtb_deps = deps.clone();
        ui.on_move_to_bottom(move |id: i32| {
            let deps = mtb_deps.clone();
            let view = helpers::current_view_state(&deps.ui);
            move_clip(deps, view, id, helpers::RowMove::Bottom, None);
        });
    }
    {
        let muo_deps = deps.clone();
        ui.on_move_up_one(move |id: i32| {
            let deps = muo_deps.clone();
            let view = helpers::current_view_state(&deps.ui);
            move_clip(deps, view, id, helpers::RowMove::Up, None);
        });
    }
    {
        let mdo_deps = deps.clone();
        ui.on_move_down_one(move |id: i32| {
            let deps = mdo_deps.clone();
            let view = helpers::current_view_state(&deps.ui);
            move_clip(deps, view, id, helpers::RowMove::Down, None);
        });
    }

    // ── Transform clip ───────────────────────────────────────────────────
    {
        let transform_deps = deps.clone();
        let transform_settings = settings.clone();
        ui.on_transform_clip(move |id: i32, key: slint::SharedString| {
            let deps = transform_deps.clone();
            let key = key.to_string();
            let keep_open = !transform_settings.borrow().always_close_to_tray;
            tokio::spawn(async move {
                match deps
                    .db
                    .with(|conn| cliptoo_core::db::queries::get_clip_content(conn, id as i64))
                    .await
                {
                    Ok(content) => {
                        let transformed = cliptoo_core::transform::transform(&content, &key);
                        if let Err(e) = crate::paste::paste_content(
                            &transformed,
                            "text",
                            &deps.suppression,
                            &deps.ui,
                            false,
                            keep_open,
                        )
                        .await
                        {
                            tracing::error!("Transform paste error (key={key}): {e}");
                            show_paste_error(&deps.ui, &e);
                        }
                    }
                    Err(e) => report_missing_clip(&deps, id, &e),
                }
            });
        });
    }

    // ── Send to app ──────────────────────────────────────────────────────
    {
        let sendto_deps = deps.clone();
        let sendto_settings = settings.clone();
        ui.on_send_to_clip(move |id: i32, app_path: slint::SharedString| {
            let deps = sendto_deps.clone();
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
                match deps
                    .db
                    .with(|conn| cliptoo_core::db::queries::get_clip_content(conn, id as i64))
                    .await
                {
                    Ok(content) => {
                        if let Err(e) =
                            cliptoo_core::send_to::send_to(&resolved_path, &content).await
                        {
                            tracing::error!("Send-to error (app='{resolved_path}'): {e}");
                            let ui = deps.ui.clone();
                            let message = format!("Could not open {resolved_path}: {e:#}");
                            let _ = ui.upgrade_in_event_loop(move |ui| {
                                show_main_toast(&ui, &message, "error");
                            });
                        }
                    }
                    Err(e) => report_missing_clip(&deps, id, &e),
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
            // Consume the selection so the id cannot linger as a stale target
            // for later compares; "Set as left" must be chosen again for a
            // fresh pair.
            cl.set(None);
            tracing::debug!("Compare: left {left_id} vs right {id}");
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

    // ── Load the next page when the list nears its end ──────────────────
    // One page in flight at a time: a wheel fling runs the list's near-end
    // check on several ticks, and two queries at the same offset would append
    // the same rows twice. `helpers::load_more_clips_live` clears the flag.
    {
        let more_pending = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let more_deps = deps.clone();
        ui.on_load_more(move || {
            if more_pending.swap(true, std::sync::atomic::Ordering::Relaxed) {
                return;
            }
            helpers::load_more_clips_live(
                &more_deps.db,
                &more_deps.ui,
                &more_deps.thumbs,
                &more_deps.favicons,
                Some(&more_deps.tag_prefix),
                more_pending.clone(),
            );
        });
    }

    // ── Item activated (paste on click / Enter) ─────────────────────────
    {
        let paste_deps = deps.clone();
        let paste_settings = settings.clone();
        ui.on_item_activated(move |id: i32| {
            let deps = paste_deps.clone();
            let settings = paste_settings.clone();
            // Read on the UI thread (Rc<RefCell> is not Send); the settings are
            // consumed as plain bools inside the spawned task. Re-read per
            // activation so a setting change applies without a restart.
            let moves_top = settings.borrow().paste_moves_clip_to_top;
            let paste_plain = settings.borrow().paste_as_plain_text;
            let keep_open = !settings.borrow().always_close_to_tray;
            let view = helpers::current_view_state(&deps.ui);
            tokio::spawn(paste_clip(
                deps,
                view,
                id,
                paste_plain,
                moves_top,
                keep_open,
            ));
        });
    }

    // ── Paste as rich/plain (per-clip override from the context menu) ─────
    // RTF/HTML clips expose the rich/plain pair; file clips expose only
    // "plain", which pastes the path as text instead of offering
    // `text/uri-list`. The choice bypasses the global "paste as plain text"
    // toggle for this one paste.
    {
        let pas_deps = deps.clone();
        let pas_settings = settings.clone();
        ui.on_paste_as(move |id: i32, mode: slint::SharedString| {
            let deps = pas_deps.clone();
            // Same bookkeeping as a normal paste: always count it, and reorder
            // the list when the "paste moves clip to top" setting is on.
            let moves_top = pas_settings.borrow().paste_moves_clip_to_top;
            let keep_open = !pas_settings.borrow().always_close_to_tray;
            let paste_plain = mode == "plain";
            let view = helpers::current_view_state(&deps.ui);
            tokio::spawn(paste_clip(
                deps,
                view,
                id,
                paste_plain,
                moves_top,
                keep_open,
            ));
        });
    }
}

/// Reorder a clip: the DB timestamp first, then the matching in-place model
/// move (or a re-query when the view is filtered or paged). `copy_rich` is set
/// for "move to top", which also loads the clip so Ctrl+V pastes it.
fn move_clip(
    deps: ClipDeps,
    view: helpers::ViewState,
    id: i32,
    how: helpers::RowMove,
    copy_rich: Option<bool>,
) {
    let in_place = deps
        .ui
        .upgrade()
        .is_some_and(|ui| helpers::model_is_timestamp_ordered(&ui));
    tokio::spawn(async move {
        let db_ok = deps
            .db
            .with(|conn| match how {
                helpers::RowMove::Top => cliptoo_core::db::queries::bump_to_top(conn, id as i64),
                helpers::RowMove::Bottom => {
                    cliptoo_core::db::queries::bump_to_bottom(conn, id as i64)
                }
                helpers::RowMove::Up => cliptoo_core::db::queries::move_up_one(conn, id as i64),
                helpers::RowMove::Down => cliptoo_core::db::queries::move_down_one(conn, id as i64),
            })
            .await
            .is_ok();
        if let Some(rich) = copy_rich {
            // Mirrors the paste path; the hash suppression keeps the listener
            // from re-ingesting our own write.
            let result = deps
                .db
                .with(|conn| cliptoo_core::db::queries::get_clip_type_and_content(conn, id as i64))
                .await;
            if let Ok((content, clip_type, _hash)) = result {
                crate::paste::register_suppression(&deps.suppression, &content, &clip_type);
                if let Err(e) =
                    crate::paste::write_content_to_clipboard(&content, &clip_type, rich).await
                {
                    tracing::error!("Move-to-top clipboard write error: {e}");
                }
            }
        }
        if in_place && db_ok {
            let _ = deps.ui.upgrade_in_event_loop(move |ui| {
                helpers::apply_row_move(&ui, id, how);
            });
        } else {
            deps.refresh(&view).await;
        }
    });
}

/// Paste one clip: count it, write the clipboard, then refresh the list. The
/// refresh runs last so the paste keystroke is not delayed by rebuilding up to
/// a page of thumbnail images.
async fn paste_clip(
    deps: ClipDeps,
    view: helpers::ViewState,
    id: i32,
    paste_plain: bool,
    moves_top: bool,
    keep_open: bool,
) {
    let _ = deps
        .db
        .with(|conn| cliptoo_core::db::queries::record_paste(conn, id as i64, moves_top))
        .await;
    let result = deps
        .db
        .with(|conn| cliptoo_core::db::queries::get_clip_type_and_content(conn, id as i64))
        .await;
    match result {
        Ok((content, clip_type, _hash)) => {
            if let Err(e) = crate::paste::paste_content(
                &content,
                &clip_type,
                &deps.suppression,
                &deps.ui,
                paste_plain,
                keep_open,
            )
            .await
            {
                tracing::error!("Paste error: {e}");
                show_paste_error(&deps.ui, &e);
            }
        }
        // A retention pass can delete a row while the window still lists it;
        // re-query so the row disappears instead of the click doing nothing.
        Err(e) => {
            report_missing_clip(&deps, id, &e);
            return;
        }
    }
    if moves_top {
        deps.refresh(&view).await;
    }
}

/// A clip the window still lists was deleted from the DB (scheduled
/// retention). Re-query the current view and tell the user why nothing pasted.
fn report_missing_clip(deps: &ClipDeps, id: i32, error: &anyhow::Error) {
    tracing::warn!("clip {id} is no longer in the database: {error}");
    helpers::refresh_clips_live(
        &deps.db,
        &deps.ui,
        &deps.thumbs,
        &deps.favicons,
        Some(&deps.tag_prefix),
    );
    let _ = deps.ui.upgrade_in_event_loop(|ui| {
        show_main_toast(&ui, "Clip no longer exists", "warn");
    });
}

/// Show a toast on the main window. Severity is "info" | "warn" | "error".
fn show_main_toast(ui: &crate::AppWindow, message: &str, severity: &str) {
    ui.set_toast_visible(false);
    ui.set_toast_message(message.into());
    ui.set_toast_severity(severity.into());
    ui.set_toast_visible(true);
}

/// A paste failed, likely after the window was already hidden for it. Bring
/// the window back and show the error; a log line alone leaves the user with a
/// vanished window and no paste.
fn show_paste_error(ui: &slint::Weak<crate::AppWindow>, error: &anyhow::Error) {
    let message = format!("Paste failed: {error:#}");
    let ui = ui.clone();
    let _ = ui.upgrade_in_event_loop(move |ui| {
        crate::window::summon_window(&ui);
        show_main_toast(&ui, &message, "error");
    });
}

/// Read a clip's file for the editor. Only regular files up to
/// `MAX_EDIT_FILE_BYTES` are read: a copied FIFO or device never ends, and a
/// multi-GB log would fill memory just to open a text box. Returns the text and
/// the mtime/size snapshot used to detect an external edit before saving.
fn load_text_file(path: &str) -> Option<(String, f64, u64)> {
    let metadata = std::fs::metadata(path).ok()?;
    if !metadata.is_file() || metadata.len() > MAX_EDIT_FILE_BYTES {
        return None;
    }
    let text = std::fs::read_to_string(path).ok()?;
    let mtime = metadata
        .modified()
        .ok()?
        .duration_since(std::time::UNIX_EPOCH)
        .ok()?
        .as_secs_f64();
    Some((text, mtime, metadata.len()))
}

/// Largest file the editor loads, and the largest one Save may rewrite.
const MAX_EDIT_FILE_BYTES: u64 = 16 * 1024 * 1024;
