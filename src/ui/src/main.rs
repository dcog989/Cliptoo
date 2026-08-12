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
mod dbus;
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

    if settings.start_with_system {
        if let Err(e) = autostart::ensure_autostart() {
            tracing::warn!("autostart: failed to create desktop file: {e}");
        }
    } else if let Err(e) = autostart::remove_autostart() {
        // Reconcile a stale autostart file (e.g. one left behind after the
        // setting was turned off elsewhere) so Cliptoo doesn't start on the
        // next login against the user's preference.
        tracing::warn!("autostart: failed to remove stale desktop file: {e}");
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

    // Watch channel: the settings UI publishes retention changes here; the
    // scheduled maintenance task reads the latest values on each pass.
    let (retention_tx, retention_rx) =
        tokio::sync::watch::channel(cliptoo_core::maintenance::RetentionConfig {
            max_clips: settings.borrow().max_clips,
            max_age_days: settings.borrow().max_age_days,
        });

    // Shared blacklist state: the settings UI replaces it on change; the
    // clipboard listener reads it on every poll, so edits apply live.
    let blacklist_state = std::sync::Arc::new(std::sync::Mutex::new(
        settings.borrow().blacklisted_apps.clone(),
    ));

    window::setup_drag(&ui);
    window::setup_resize(&ui);
    window::setup_close_handlers(&ui, &settings, &dirs);
    window::setup_close_to_tray(&ui);
    window::setup_focus_regained(&ui);

    let settings_win = settings::setup_settings_window(
        &ui,
        &settings,
        &dirs,
        hotkey_tx,
        retention_tx,
        blacklist_state.clone(),
    );
    // Slint globals are per-window-instance: the settings window has its own
    // `Theme` global that must be filled separately from the main window's.
    theme::fill_theme(
        &settings_win.global::<crate::Theme>(),
        &settings.borrow(),
        is_dark,
        system_accent,
    );

    let about_win = about::setup_about_window(&ui, &dirs);
    theme::fill_theme(
        &about_win.global::<crate::Theme>(),
        &settings.borrow(),
        is_dark,
        system_accent,
    );

    stats_ui::setup_stats(&settings_win, &db, &dirs.db_path);

    // Mirror of the toolbar's active clip-type filter. Slint properties are
    // only readable on the UI thread, so background tasks (the clipboard
    // listener) read this instead of the live property; setup_filter keeps it
    // in sync on every filter change.
    let active_filter_state: Arc<std::sync::Mutex<String>> =
        Arc::new(std::sync::Mutex::new(String::from("all")));

    search::setup_search(&ui, &db, &dirs, &tag_prefix);

    search::setup_filter(&ui, &db, &dirs, &active_filter_state);

    preview::setup_preview(&ui, &db, &dirs);
    preview::setup_dismiss_preview(&ui);

    let edit_win = edit::setup_edit_window(&ui, &settings, &dirs, &db, &tag_prefix);
    theme::fill_theme(
        &edit_win.global::<crate::Theme>(),
        &settings.borrow(),
        is_dark,
        system_accent,
    );

    let suppression = Arc::new(paste::PasteSuppressionSet::new());
    actions::setup_clip_actions(
        &ui,
        &edit_win,
        &db,
        &settings,
        &dirs,
        &suppression,
        &tag_prefix,
    );

    const MAINTENANCE_INTERVAL_SECS: u64 = 6 * 60 * 60;
    cliptoo_core::maintenance::spawn_scheduler(
        db.clone(),
        dirs.thumbnails_dir.clone(),
        dirs.favicons_dir.clone(),
        retention_rx,
        MAINTENANCE_INTERVAL_SECS,
    );
    maintenance::setup_manual_maintenance(&ui, &db, &dirs, &settings, &settings_win);

    // ── Clipboard listener ─────────────────────────────────────────────
    clipboard::spawn_listener(
        db.clone(),
        ui.as_weak(),
        dirs.thumbnails_dir.clone(),
        dirs.favicons_dir.clone(),
        dirs.images_dir.clone(),
        suppression.clone(),
        blacklist_state.clone(),
        settings.borrow().hover_image_preview_size,
        active_filter_state.clone(),
    );

    // Populate the clip list from history on startup. The listener no longer
    // ingests the pre-existing clipboard at launch (it seeds the change-detection
    // baseline instead), so that startup read no longer doubles as the initial
    // list refresh; without this the list stays empty until the first new copy.
    {
        let db = db.clone();
        let ui_weak = ui.as_weak();
        let td = dirs.thumbnails_dir.clone();
        let fd = dirs.favicons_dir.clone();
        let filter = active_filter_state.lock().unwrap().clone();
        tokio::spawn(async move {
            helpers::refresh_clips(&db, &ui_weak, &td, &fd, "", &filter, None).await;
        });
    }

    // ── Global shortcuts ───────────────────────────────────────────────
    {
        let ui_weak = ui.as_weak();
        tokio::spawn(async move {
            hotkeys::run_hotkey_loop(ui_weak, hotkey_rx).await;
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
                        window::toggle_window(&w);
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
    // When "Always close to tray" is off, start with the main window visible.
    let start_hidden = _tray.is_some() && settings.borrow().always_close_to_tray;
    if start_hidden {
        window::hide_window(&ui);
    } else {
        window::show_window(&ui)?;
    }
    slint::run_event_loop_until_quit()?;

    info!("Cliptoo exiting");
    Ok(())
}
