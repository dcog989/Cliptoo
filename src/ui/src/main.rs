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
mod decode_limit;
mod drag;
mod edit;
mod favicon;
mod helpers;
mod hotkeys;
mod instance;
mod lazy_window;
mod maintenance;
mod paste;
mod positioning;
mod preview;
mod qr;
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
    // Mirror the preview size for list-driven thumbnail generation (see
    // `thumbnail_cache::check_pending_thumbnails`).
    thumbnail_cache::set_preview_max_dim(settings.hover_image_preview_size);
    let _log_guard = cliptoo_core::logger::init(&dirs.logs_dir, settings.log_level_filter());

    // reqwest is built with `rustls-no-provider`, so it reads the process-wide
    // provider and panics on the first `Client::build()` if none is installed.
    // Must run before any client is constructed; an already-installed provider
    // is fine.
    let _ = rustls::crypto::ring::default_provider().install_default();

    info!("Cliptoo starting");

    // A second instance would fight the first over the fixed-name KWin tracker
    // script and register a duplicate global shortcut, so only the first runs.
    // A lock file that cannot be created must not stop the app from starting.
    match instance::acquire(&dirs.lock_path) {
        Ok(true) => {}
        Ok(false) => {
            info!("another Cliptoo instance is already running; exiting");
            return Ok(());
        }
        Err(e) => tracing::warn!("single-instance lock unavailable: {e}"),
    }

    // After the instance check: a second copy must not rewrite the cache files
    // the running instance is reading. Cached hover previews from an older
    // pixel ratio are upscaled (blurred) in the popup; drop them so the next
    // hover regenerates at the current ratio.
    cliptoo_core::image::migrate_preview_ratio(&dirs.thumbnails_dir);

    // Build the Ctrl+V virtual keyboard now rather than on the first paste:
    // evdev drops events for a device the compositor has not opened yet, which
    // made the first paste of a session fail silently. See paste.rs.
    paste::init_virtual_keyboard();

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

    // App-wide: scroll by the wheel's angleDelta instead of Wayland's tiny
    // pixelDelta, which otherwise makes one notch scroll only a few pixels.
    drag::normalize_wheel_delta();
    // App-wide: complete a click whose press the compositor consumed while
    // activating the inactive window, so the first click registers.
    drag::repair_activation_clicks();
    // App-wide: dismiss the hover preview on any left click, including a click
    // on an already-focused search field that emits no focus change.
    drag::dismiss_preview_on_left_press();

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

    // Shared hover-image-preview size: the settings UI updates it on change;
    // the clipboard listener reads it per thumbnail generation, so the setting
    // applies to newly ingested images without a restart.
    let image_preview_size = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(
        settings.borrow().hover_image_preview_size,
    ));

    window::setup_resize(&ui);
    window::setup_close_handlers(&ui, &settings, &dirs);
    window::setup_close_to_tray(&ui);
    window::setup_focus_regained(&ui);
    // Keep the timer alive for the app's lifetime.
    let _window_active_poll = window::setup_window_active_poll(&ui);

    // Windows other than the main/settings ones (tray, About) hold their own
    // `Theme` global; register them here so a live theme/accent change reaches
    // them too (see theme::ThemeFillers).
    let theme_fillers = theme::new_theme_fillers();

    // Settings, About, Edit and QR are built on first use (see lazy_window):
    // creating them at startup costs their whole scene graph for a window most
    // sessions never open. Only the main, tray and hover-preview windows exist
    // here.
    let settings_slot = lazy_window::LazyWindow::<SettingsWindow>::new();
    settings::register_settings(
        &ui,
        &settings_slot,
        settings.clone(),
        dirs.clone(),
        db.clone(),
        hotkey_tx,
        retention_tx,
        blacklist_state.clone(),
        image_preview_size.clone(),
        theme_fillers.clone(),
    );
    // Follow live OS appearance changes (KDE/XDG portal `SettingChanged`).
    settings::theme_apply::watch_system_theme(
        ui.as_weak(),
        settings_slot,
        settings.clone(),
        dirs.favicons_dir.clone(),
        db.clone(),
        theme_fillers.clone(),
    );

    let about_slot = lazy_window::LazyWindow::<AboutWindow>::new();
    about::register_about(
        &ui,
        &about_slot,
        dirs.clone(),
        settings.clone(),
        theme_fillers.clone(),
    );

    // Standalone hover-preview surface. It is re-parented onto the main window
    // as an input-transparent, focus-less Qt tooltip (see drag.rs) so a preview
    // can be larger than the main window without stealing focus (which would
    // close the main window to the tray) or the pointer hover that opened it.
    let preview_win = PreviewWindow::new()?;
    theme::fill_theme(
        &preview_win.global::<crate::Theme>(),
        &settings.borrow(),
        is_dark,
        system_accent,
    );
    theme::register_preview_theme_filler(&theme_fillers, &preview_win.as_weak());
    drag::make_tooltip_window(&preview_win, &ui);
    preview::register_preview_window(&preview_win);

    search::setup_search(&ui, &db, &dirs, &tag_prefix);

    search::setup_filter(&ui, &db, &dirs);

    preview::setup_preview(&ui, &preview_win, &db, &dirs, image_preview_size.clone());
    preview::setup_dismiss_preview(&ui, &preview_win);

    let edit_slot = lazy_window::LazyWindow::<EditWindow>::new();
    let qr_slot = lazy_window::LazyWindow::<QrWindow>::new();

    let suppression = Arc::new(paste::PasteSuppressionSet::new());
    qr::setup_qr(
        &ui,
        &qr_slot,
        &db,
        &suppression,
        settings.clone(),
        theme_fillers.clone(),
    );
    actions::setup_clip_actions(
        &ui,
        &edit_slot,
        &db,
        &settings,
        &dirs,
        &suppression,
        &tag_prefix,
        theme_fillers,
    );

    const MAINTENANCE_INTERVAL_SECS: u64 = 6 * 60 * 60;
    cliptoo_core::maintenance::spawn_scheduler(
        db.clone(),
        dirs.thumbnails_dir.clone(),
        dirs.favicons_dir.clone(),
        dirs.images_dir.clone(),
        retention_rx,
        MAINTENANCE_INTERVAL_SECS,
    );

    // ── Clipboard listener ─────────────────────────────────────────────
    // Active-window tracker: a resident KWin script keeps a shared cache of
    // the focused app; the clipboard watcher snapshots it at the exact moment
    // a copy lands, so the recorded source app is the app the user copied
    // *from* even when focus moves on before the listener ingests.
    let active_window_cache = source_app::spawn_active_window_tracker();
    let (selection_tx, selection_rx) =
        tokio::sync::mpsc::channel(clipboard::SELECTION_CHANNEL_CAPACITY);
    let watcher_connected =
        clipboard::spawn_clipboard_watcher(active_window_cache.clone(), selection_tx);
    clipboard::spawn_listener(
        db.clone(),
        ui.as_weak(),
        dirs.thumbnails_dir.clone(),
        dirs.favicons_dir.clone(),
        dirs.images_dir.clone(),
        suppression.clone(),
        blacklist_state.clone(),
        image_preview_size.clone(),
        selection_rx,
        active_window_cache,
        watcher_connected,
    );

    // The clip list is populated by `show_window` (AppWindow `refresh-clips`):
    // background refreshes are skipped while the window is hidden, so the show
    // path is the one that fills the list from history.

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
            {
                let win = ui.as_weak();
                tray.on_toggle_window(move || {
                    if let Some(w) = win.upgrade() {
                        window::toggle_window(&w);
                    }
                });
            }

            // Quit via the event loop so `main` returns cleanly: the log
            // guard is flushed on drop and the "Cliptoo exiting" line is
            // emitted, instead of a hard exit that skips teardown.
            let quit_settings = settings.clone();
            let quit_settings_path = dirs.settings_path.clone();
            tray.on_quit_app(move || {
                // Flush the debounced settings write before the event loop
                // stops, or the last edit is lost when quitting from the tray.
                let _ = quit_settings.borrow().save(&quit_settings_path);
                let _ = slint::quit_event_loop();
            });

            // Slint 1.18 reports a missing system tray backend from show().
            // Only keep the tray when it actually appeared: otherwise
            // start_hidden below would hide the window behind a tray icon that
            // isn't there, leaving the app unreachable.
            match tray.show() {
                Ok(()) => _tray = Some(tray),
                Err(e) => {
                    tracing::warn!("System tray unavailable, keeping the window visible: {e}")
                }
            }
        }
        Err(e) => tracing::warn!("System tray unavailable (app will still work): {e}"),
    }

    // Start closed to tray. Only fall back to showing the window when the
    // tray failed to initialise or show, otherwise the app would be unreachable.
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

#[cfg(test)]
mod tests {
    /// Fails if a bump ever puts reqwest and rustls on different versions:
    /// reqwest's client then panics on the missing process provider.
    #[test]
    fn rustls_provider_installed_for_reqwest() {
        let _ = rustls::crypto::ring::default_provider().install_default();
        assert!(reqwest::Client::builder().build().is_ok());
    }
}
