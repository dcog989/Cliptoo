// No `unsafe` anywhere in this crate except the Qt FFI shim in `drag.rs`
// (which carries a module-level `#![allow(unsafe_code)]`).
#![deny(unsafe_code)]

use anyhow::Result;
use slint::VecModel;
use std::sync::Arc;
use tracing::info;

mod about;
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
mod stats_ui;
mod theme;
mod thumbnail_cache;

mod window;

slint::include_modules!();

#[tokio::main]
async fn main() -> Result<()> {
    let dirs = app_dirs::AppDirs::resolve()?;
    let settings = cliptoo_core::Settings::load(&dirs.settings_path);
    let _log_guard = cliptoo_core::logger::init(&dirs.logs_dir, settings.log_level_filter());

    info!("Cliptoo starting");

    if settings.start_with_system
        && let Err(e) = autostart::ensure_autostart()
    {
        tracing::warn!("autostart: failed to create desktop file: {e}");
    }

    let db = Arc::new(cliptoo_core::db::DbPool::open(&dirs.db_path)?);
    let ui = AppWindow::new()?;

    ui.set_clips(std::rc::Rc::new(VecModel::<ClipData>::from(vec![])).into());
    let (is_dark, system_accent) = theme::apply_theme(&ui, &settings).await;
    ui.set_stored_width(settings.window_width as f32);
    ui.set_stored_height(settings.window_height as f32);
    ui.set_quick_paste_mod(settings.quick_paste_modifier.clone().into());

    let settings = std::rc::Rc::new(std::cell::RefCell::new(settings));
    let tag_prefix = settings.borrow().tag_prefix.clone();

    // Watch channel: the settings UI publishes hotkey changes here; the global
    // shortcut registration loop below re-registers when they change.
    let (hotkey_tx, hotkey_rx) = tokio::sync::watch::channel(settings.borrow().hotkey.clone());

    window::setup_drag(&ui);
    window::setup_resize(&ui);
    window::setup_close_handlers(&ui, &settings, &dirs);
    window::setup_close_to_tray(&ui);

    let settings_win = settings::setup_settings_window(&ui, &settings, &dirs, hotkey_tx);
    // Slint globals are per-window-instance: the settings window has its own
    // `Theme` global that must be filled separately from the main window's.
    theme::fill_theme(
        &settings_win.global::<crate::Theme>(),
        &settings.borrow(),
        is_dark,
        system_accent,
    );

    let about_win = about::setup_about_window(&ui, &dirs.logs_dir);
    theme::fill_theme(
        &about_win.global::<crate::Theme>(),
        &settings.borrow(),
        is_dark,
        system_accent,
    );

    stats_ui::setup_stats(&settings_win, &db, &dirs.db_path);

    search::setup_search(&ui, &db, &dirs, &tag_prefix);

    search::setup_filter(&ui, &db, &dirs);

    preview::setup_preview(&ui, &db, &dirs);
    preview::setup_dismiss_preview(&ui);

    let edit_win = edit::setup_edit_window(&ui, &settings, &dirs, &db);
    theme::fill_theme(
        &edit_win.global::<crate::Theme>(),
        &settings.borrow(),
        is_dark,
        system_accent,
    );

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
    maintenance::setup_manual_maintenance(&ui, &db, &dirs, &settings, &settings_win);

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
        let ui_weak = ui.as_weak();
        let mut hotkey_rx = hotkey_rx;
        tokio::spawn(async move {
            loop {
                let main_hotkey = hotkey_rx.borrow().clone();

                hotkeys::check_portal_presence().await;

                let handle = hotkeys::register_shortcuts_and_listen(
                    &[("toggle-cliptoo", main_hotkey.as_str())],
                    {
                        let weak = ui_weak.clone();
                        move |shortcut_id| {
                            if shortcut_id == "toggle-cliptoo" {
                                let _ = weak.upgrade_in_event_loop(move |ui| {
                                    use slint::ComponentHandle;
                                    if ComponentHandle::window(&ui).is_visible() {
                                        window::hide_window(&ui);
                                    } else {
                                        let _ = window::show_window(&ui);
                                    }
                                });
                            }
                        }
                    },
                )
                .await;

                if let Err(e) = &handle {
                    tracing::warn!("Global shortcuts unavailable: {e}");
                }

                // Wait for the user to change a hotkey in Settings. The
                // settings UI commits on every key-press, so a user typing
                // `Ctrl+Alt+Q` fires several changes in quick succession.
                // Debounce: only act once the value has been stable for a
                // quiet period, so the KDE confirmation dialog appears for
                // the complete combo, not the first modifier key.
                const HOTKEY_DEBOUNCE_MS: u64 = 800;
                if hotkey_rx.changed().await.is_err() {
                    break;
                }
                loop {
                    match tokio::time::timeout(
                        std::time::Duration::from_millis(HOTKEY_DEBOUNCE_MS),
                        hotkey_rx.changed(),
                    )
                    .await
                    {
                        Err(_) => break,
                        Ok(Err(_)) => break,
                        Ok(Ok(())) => continue,
                    }
                }

                // Drop the old listener, clear the stale KGlobalAccel keys so
                // the portal treats the shortcut as new (and applies the new
                // preferred_trigger), then loop to re-register.
                handle.map(|h| h.abort()).ok();
                hotkeys::clear_kglobalaccel_bindings(&["toggle-cliptoo"]).await;
            }
        });
    }

    // ── System tray ────────────────────────────────────────────────────
    let mut _tray = None;
    match CliptooTray::new() {
        Ok(tray) => {
            // SystemTrayIcon has its own global scope; init Theme on it too.
            theme::fill_theme(
                &tray.global::<crate::Theme>(),
                &settings.borrow(),
                is_dark,
                system_accent,
            );

            {
                let win = ui.as_weak();
                tray.on_toggle_window(move || {
                    if let Some(w) = win.upgrade() {
                        use slint::ComponentHandle;
                        if w.window().is_visible() {
                            window::hide_window(&w);
                        } else {
                            let _ = window::show_window(&w);
                        }
                    }
                });
            }

            tray.on_quit_app(move || std::process::exit(0));

            let _ = tray.show();
            _tray = Some(tray);
        }
        Err(e) => tracing::warn!("System tray unavailable (app will still work): {e}"),
    }

    // Start closed to tray. Only fall back to showing the window when the
    // tray failed to initialise, otherwise the app would be unreachable.
    if _tray.is_some() {
        window::hide_window(&ui);
    } else {
        window::show_window(&ui)?;
    }
    slint::run_event_loop_until_quit()?;

    info!("Cliptoo exiting");
    Ok(())
}
