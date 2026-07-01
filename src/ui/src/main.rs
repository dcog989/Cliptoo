use anyhow::Result;
use slint::Model;
use slint::VecModel;
use std::sync::Arc;
use tracing::info;

mod actions;
mod app_dirs;
mod autostart;
mod clipboard;
mod drag;
mod edit;
mod favicon;
mod helpers;
mod hotkeys;
mod maintenance;
mod paste;
mod positioning;
mod preview;
mod search;
mod settings;
mod source_app;
mod theme;
mod thumbnail_cache;
mod tray;
mod window;

slint::include_modules!();

#[tokio::main]
async fn main() -> Result<()> {
    let dirs = app_dirs::AppDirs::resolve()?;
    let settings = cliptoo_core::Settings::load(&dirs.settings_path);
    let _log_guard = cliptoo_core::logger::init(
        &dirs.logs_dir,
        settings.log_level_filter(),
        settings.log_retention_days,
    );

    info!("Cliptoo starting");

    if settings.start_with_system
        && let Err(e) = autostart::ensure_autostart()
    {
        tracing::warn!("autostart: failed to create desktop file: {e}");
    }

    let db = Arc::new(cliptoo_core::db::DbPool::open(&dirs.db_path)?);
    let ui = AppWindow::new()?;

    ui.set_clips(std::rc::Rc::new(VecModel::<ClipData>::from(vec![])).into());
    theme::apply_theme(&ui, &settings).await;
    ui.set_stored_width(settings.window_width as f32);
    ui.set_stored_height(settings.window_height as f32);

    let settings = std::rc::Rc::new(std::cell::RefCell::new(settings));
    let tag_prefix = settings.borrow().tag_prefix.clone();

    window::setup_drag(&ui);
    window::setup_resize(&ui);
    window::setup_close_handlers(&ui, &settings, &dirs);
    window::setup_close_to_tray(&ui);

    let _settings_win = settings::setup_settings_window(&ui, &settings, &dirs);

    search::setup_search(&ui, &db, &dirs, &tag_prefix);

    search::setup_filter(&ui, &db, &dirs);

    preview::setup_preview(&ui, &db, &dirs);
    preview::setup_dismiss_preview(&ui);

    let edit_win = edit::setup_edit_window(&ui, &settings, &dirs, &db);

    let suppression = Arc::new(paste::PasteSuppressionSet::new());
    actions::setup_clip_actions(&ui, &edit_win, &db, &settings, &dirs, &suppression);

    const MAINTENANCE_INTERVAL_SECS: u64 = 6 * 60 * 60;
    {
        let s = settings.borrow();
        cliptoo_core::maintenance::spawn_scheduler(
            db.clone(),
            dirs.thumbnails_dir.clone(),
            dirs.favicons_dir.clone(),
            s.max_clips,
            s.max_age_days,
            MAINTENANCE_INTERVAL_SECS,
        );
    }
    maintenance::setup_manual_maintenance(&ui, &db, &dirs, &settings);

    // ── Clipboard listener ─────────────────────────────────────────────
    {
        let db = db.clone();
        let ui_weak = ui.as_weak();
        let td = dirs.thumbnails_dir.clone();
        let fd = dirs.favicons_dir.clone();
        let id = dirs.images_dir.clone();
        let sup = suppression.clone();
        let blacklist = settings.borrow().blacklisted_apps.clone();
        let preview_max_dim = settings.borrow().hover_image_preview_size;
        tokio::spawn(async move {
            if let Err(e) =
                clipboard::run_listener(db, ui_weak, td, fd, id, sup, blacklist, preview_max_dim)
                    .await
            {
                tracing::error!("Clipboard listener error: {e}");
            }
        });
    }

    // ── Global shortcuts ───────────────────────────────────────────────
    {
        let pos_settings = positioning::PositionSettings::from(&*settings.borrow());
        let main_hotkey = settings.borrow().hotkey.clone();
        let preview_hotkey = settings.borrow().preview_hotkey.clone();
        let ui_weak = ui.as_weak();
        tokio::spawn(async move {
            hotkeys::check_portal_presence().await;
            if let Err(e) = hotkeys::register_shortcuts_and_listen(
                &[
                    ("toggle-cliptoo", main_hotkey.as_str()),
                    ("preview-cliptoo", preview_hotkey.as_str()),
                ],
                move |shortcut_id| {
                    let ps = pos_settings.clone();
                    match shortcut_id.as_str() {
                        "toggle-cliptoo" => {
                            let _ = ui_weak.upgrade_in_event_loop(move |ui| {
                                use slint::ComponentHandle;
                                if ComponentHandle::window(&ui).is_visible() {
                                    let _ = ComponentHandle::hide(&ui);
                                } else {
                                    let _ = ComponentHandle::show(&ui);
                                    positioning::position_window_ex(&ui, &ps);
                                }
                            });
                        }
                        "preview-cliptoo" => {
                            let _ = ui_weak.upgrade_in_event_loop(move |ui| {
                                let idx = ui.get_selected_index();
                                let visible = ui.get_preview_visible();
                                if visible {
                                    ui.set_preview_visible(false);
                                } else if let Some(clip) = ui.get_clips().row_data(idx as usize) {
                                    let row_h = ui.global::<crate::Theme>().get_row_height();
                                    let preview_x = 8.0_f32;
                                    let preview_y = (idx as f32 * row_h) + row_h;
                                    ui.set_preview_popup_x(preview_x);
                                    ui.set_preview_popup_y(preview_y);
                                    ui.invoke_request_preview(clip.id, preview_x, preview_y);
                                }
                            });
                        }
                        _ => {}
                    }
                },
            )
            .await
            {
                tracing::warn!("Global shortcuts unavailable: {e}");
            }
        });
    }

    // ── System tray ────────────────────────────────────────────────────
    let _tray_handle;
    {
        let tray_pos = positioning::PositionSettings::from(&*settings.borrow());
        let tray_ui = ui.as_weak();
        let (action_tx, mut action_rx) = tokio::sync::mpsc::unbounded_channel::<tray::TrayAction>();

        tokio::spawn(async move {
            while let Some(action) = action_rx.recv().await {
                let ui = tray_ui.clone();
                let ps = tray_pos.clone();
                let _ = ui.upgrade_in_event_loop(move |ui| {
                    use slint::ComponentHandle;
                    match action {
                        tray::TrayAction::ToggleWindow => {
                            if ComponentHandle::window(&ui).is_visible() {
                                let _ = ComponentHandle::hide(&ui);
                            } else {
                                let _ = ComponentHandle::show(&ui);
                                positioning::position_window_ex(&ui, &ps);
                            }
                        }
                        tray::TrayAction::Quit => {
                            std::process::exit(0);
                        }
                    }
                });
            }
        });

        match tray::create_tray(action_tx).await {
            Ok(handle) => _tray_handle = handle,
            Err(e) => tracing::warn!("System tray unavailable (app will still work): {e}"),
        }
    }

    ui.show()?;
    positioning::position_window(&ui, &settings.borrow());
    slint::run_event_loop_until_quit()?;

    info!("Cliptoo exiting");
    Ok(())
}
