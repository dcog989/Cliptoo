//! Settings window wiring: construction, persisted-property seeding, close
//! persistence, and the top-level handlers that route menu/maintenance events.

use slint::ComponentHandle;

pub(crate) use self::parse::{LOG_LEVELS, idx_of};
use self::parse::{
    QUICK_PASTE_MODIFIERS, ROW_PADDINGS, THEMES, clean_hotkey_text, format_send_to_apps,
    normalize_choice,
};

mod commit;
mod parse;
mod pickers;
pub(crate) mod theme_apply;

/// Show a toast on the settings window (its bottom overlay). Severity is
/// "info" | "warn" | "error". Shared so every action reports failures the
/// same way.
pub(crate) fn show_toast(win: &slint::Weak<crate::SettingsWindow>, message: &str, severity: &str) {
    let message = message.to_string();
    let severity = severity.to_string();
    let _ = win.upgrade_in_event_loop(move |win| {
        win.set_toast_visible(false);
        win.set_toast_message(message.into());
        win.set_toast_severity(severity.into());
        win.set_toast_visible(true);
    });
}

/// Persist the settings window's current size into `Settings` so a resized
/// window keeps its size across restarts, and update the window's own
/// `stored-width`/`stored-height` so its `preferred-*` binding reflects the
/// resized geometry (otherwise re-showing the window snaps back to the stale
/// preferred size). A height the user never adjusted stays `None`, so the next
/// open follows the main window again.
fn persist_window_size(
    win: &crate::SettingsWindow,
    settings: &std::rc::Rc<std::cell::RefCell<cliptoo_core::Settings>>,
    path: &std::path::Path,
) {
    let size = win.window().size();
    win.set_stored_width(size.width as f32);
    win.set_stored_height(size.height as f32);
    let mut s = settings.borrow_mut();
    s.settings_window_width = size.width as f64;
    if (size.height as f32 - win.get_opened_height()).abs() > 1.0 {
        s.settings_window_height = Some(size.height as f64);
        s.settings_window_height_custom = true;
    }
    let _ = s.save(path);
}

/// Shared close path: persist the window size, then let the main window drop
/// its blur-to-tray guard for the settings window. Used by the window manager
/// close and the in-window close (ESC).
fn close_settings(
    win: &slint::Weak<crate::SettingsWindow>,
    main_ui: &slint::Weak<crate::AppWindow>,
    settings: &std::rc::Rc<std::cell::RefCell<cliptoo_core::Settings>>,
    path: &std::path::Path,
) {
    if let Some(win) = win.upgrade() {
        persist_window_size(&win, settings, path);
        let _ = win.hide();
    }
    if let Some(ui) = main_ui.upgrade() {
        ui.set_settings_open(false);
    }
}

/// Handle the window manager's close request. The in-window close (ESC) goes
/// through `setup_settings_closing`; both share `close_settings`.
fn setup_close_persistence(
    settings_win: &crate::SettingsWindow,
    main_ui: &crate::AppWindow,
    settings: &std::rc::Rc<std::cell::RefCell<cliptoo_core::Settings>>,
    settings_path: &std::path::Path,
) {
    let main_ui = main_ui.as_weak();
    let sw = settings_win.as_weak();
    let s = settings.clone();
    let p = settings_path.to_path_buf();
    settings_win.window().on_close_requested(move || {
        close_settings(&sw, &main_ui, &s, &p);
        slint::CloseRequestResponse::HideWindow
    });
}

/// Push every persisted setting into the window's properties.
fn init_settings_properties(
    settings_win: &crate::SettingsWindow,
    settings: &std::rc::Rc<std::cell::RefCell<cliptoo_core::Settings>>,
    settings_path: &std::path::Path,
) {
    let mut s = settings.borrow_mut();
    // A stored value that no longer matches its option list (hand edit, or an
    // option renamed between versions) is rewritten to the fallback option, so
    // the dropdown and the behaviour that reads the string cannot disagree.
    let mut rewritten = false;
    rewritten |= normalize_choice(&mut s.quick_paste_modifier, &QUICK_PASTE_MODIFIERS);
    rewritten |= normalize_choice(&mut s.logging_level, &LOG_LEVELS);
    rewritten |= normalize_choice(&mut s.theme, &THEMES);
    rewritten |= normalize_choice(&mut s.clip_item_padding, &ROW_PADDINGS);
    if rewritten {
        let _ = s.save(settings_path);
    }

    settings_win.set_stored_width(s.settings_window_width as f32);
    settings_win.set_stored_height(if s.settings_window_height_custom {
        s.settings_window_height.unwrap_or(s.window_height)
    } else {
        s.window_height
    } as f32);
    let v = settings_win.global::<crate::SettingsValues>();
    v.set_s_hotkey(clean_hotkey_text(s.hotkey.as_str()).into());
    v.set_s_start_with_system(s.start_with_system);
    v.set_s_always_close_to_tray(s.always_close_to_tray);
    v.set_s_quick_paste_mod_idx(idx_of(&s.quick_paste_modifier, &QUICK_PASTE_MODIFIERS));
    v.set_s_theme_idx(idx_of(&s.theme, &THEMES));
    v.set_s_accent_color(if s.accent_color.trim().is_empty() {
        crate::theme::default_accent_color()
    } else {
        crate::theme::accent_hex_to_color(&s.accent_color)
    });
    v.set_s_accent_custom(!s.accent_color.trim().is_empty());
    v.set_s_font_family(s.font_family.as_str().into());
    v.set_s_font_size_hundredths((s.font_size * 100.0) as i32);
    v.set_s_preview_font_size_hundredths((s.preview_font_size * 100.0) as i32);
    v.set_s_row_padding_idx(idx_of(&s.clip_item_padding, &ROW_PADDINGS));
    v.set_hover_delay(s.hover_preview_delay as i32);
    v.set_s_image_preview_size(s.hover_image_preview_size as i32);
    v.set_s_color_code_clip_types(s.color_code_clip_types);
    v.set_s_paste_as_plain_text(s.paste_as_plain_text);
    v.set_s_paste_moves_to_top(s.paste_moves_clip_to_top);
    v.set_s_diff_tool_path(s.compare_tool_path.as_str().into());
    v.set_s_send_to_apps(format_send_to_apps(&s.send_to_apps).into());
    v.set_s_blacklist_apps(s.blacklisted_apps.join(", ").into());
    v.set_s_max_clips(s.max_clips as i32);
    v.set_s_max_age_days(s.max_age_days as i32);
}

/// Forward maintenance actions from the settings window to the main window.
fn setup_maintenance_forwarding(settings_win: &crate::SettingsWindow, main_ui: &crate::AppWindow) {
    let main_ui = main_ui.as_weak();
    settings_win.on_maintenance_action(move |key: slint::SharedString| {
        if let Some(ui) = main_ui.upgrade() {
            ui.invoke_maintenance_action(key);
        }
    });
}

/// When the settings window closes from inside the window (ESC), reset
/// settings-open so ESC and blur-to-tray work again on the main window, and
/// persist the size.
fn setup_settings_closing(
    settings_win: &crate::SettingsWindow,
    main_ui: &crate::AppWindow,
    settings: &std::rc::Rc<std::cell::RefCell<cliptoo_core::Settings>>,
    settings_path: &std::path::Path,
) {
    let main_ui = main_ui.as_weak();
    let sw = settings_win.as_weak();
    let s = settings.clone();
    let p = settings_path.to_path_buf();
    settings_win.on_settings_closing(move || {
        close_settings(&sw, &main_ui, &s, &p);
    });
}

/// Show the settings window from the hamburger menu.
fn present_settings(
    win: &crate::SettingsWindow,
    main_ui: &crate::AppWindow,
    settings: &std::rc::Rc<std::cell::RefCell<cliptoo_core::Settings>>,
) {
    // Guard blur-to-tray while the settings window is visible; cleared when the
    // settings window closes.
    main_ui.set_settings_open(true);
    let main_height = main_ui.get_stored_height();
    // Always reopen on the settings page (not a sub-page), regardless of where
    // the user left off.
    win.set_on_database_page(false);
    win.set_on_shortcuts_page(false);
    // Never adjusted: follow the main window's current height. An adjusted
    // height is restored from Settings instead.
    let height = {
        let s = settings.borrow();
        if s.settings_window_height_custom {
            s.settings_window_height.unwrap_or(main_height as f64) as f32
        } else {
            main_height
        }
    };
    win.set_stored_height(height);
    win.set_opened_height(height);
    // preferred-height is shrunk by the WM decorations on Wayland, so size the
    // window explicitly to open at the stored size.
    win.window().set_size(slint::LogicalSize {
        width: win.get_stored_width(),
        height,
    });
    win.show().ok();
    // Qt's show() doesn't raise a freshly-shown window; after a popup
    // (context/hamburger menu) closes it can restart under the main window.
    // Raise+activate so Settings always opens on top.
    crate::drag::activate_window(win);
}

/// Register the Settings open handler. The window itself is built on the first
/// open, not at startup, so a session that never opens it never pays for it.
#[allow(clippy::too_many_arguments)]
pub fn register_settings(
    ui: &crate::AppWindow,
    slot: &crate::lazy_window::LazyWindow<crate::SettingsWindow>,
    settings: std::rc::Rc<std::cell::RefCell<cliptoo_core::Settings>>,
    dirs: crate::app_dirs::AppDirs,
    db: std::sync::Arc<cliptoo_core::db::DbPool>,
    hotkey_tx: tokio::sync::watch::Sender<String>,
    retention_tx: tokio::sync::watch::Sender<cliptoo_core::maintenance::RetentionConfig>,
    blacklist_state: std::sync::Arc<std::sync::Mutex<Vec<String>>>,
    image_preview_size: std::sync::Arc<std::sync::atomic::AtomicU32>,
    theme_fillers: crate::theme::ThemeFillers,
) {
    let weak_ui = ui.as_weak();
    let slot = slot.clone();
    ui.on_menu_settings(move || {
        let Some(ui) = weak_ui.upgrade() else {
            return;
        };
        let win = slot.get_or_create(|| {
            create_settings_window(
                &ui,
                &settings,
                &dirs,
                &db,
                hotkey_tx.clone(),
                retention_tx.clone(),
                blacklist_state.clone(),
                image_preview_size.clone(),
                theme_fillers.clone(),
            )
        });
        present_settings(&win, &ui, &settings);
    });
}

#[allow(clippy::too_many_arguments)]
fn create_settings_window(
    ui: &crate::AppWindow,
    settings: &std::rc::Rc<std::cell::RefCell<cliptoo_core::Settings>>,
    dirs: &crate::app_dirs::AppDirs,
    db: &std::sync::Arc<cliptoo_core::db::DbPool>,
    hotkey_tx: tokio::sync::watch::Sender<String>,
    retention_tx: tokio::sync::watch::Sender<cliptoo_core::maintenance::RetentionConfig>,
    blacklist_state: std::sync::Arc<std::sync::Mutex<Vec<String>>>,
    image_preview_size: std::sync::Arc<std::sync::atomic::AtomicU32>,
    theme_fillers: crate::theme::ThemeFillers,
) -> crate::SettingsWindow {
    let settings_win = crate::SettingsWindow::new().expect("SettingsWindow creation");

    // A lazily created window missed the startup theme fill; fill it before any
    // handler reads a token.
    let (is_dark, system_accent) = crate::theme::cached_resolved_theme();
    crate::theme::fill_theme(
        &settings_win.global::<crate::Theme>(),
        &settings.borrow(),
        is_dark,
        system_accent,
    );

    setup_close_persistence(&settings_win, ui, settings, &dirs.settings_path);
    init_settings_properties(&settings_win, settings, &dirs.settings_path);
    setup_maintenance_forwarding(&settings_win, ui);
    pickers::setup_clear_accent(
        &settings_win,
        ui,
        settings,
        &dirs.settings_path,
        theme_fillers.clone(),
    );
    setup_settings_closing(&settings_win, ui, settings, &dirs.settings_path);
    pickers::setup_accent_picker(
        &settings_win,
        ui,
        settings,
        &dirs.settings_path,
        theme_fillers.clone(),
    );
    pickers::setup_font_picker(&settings_win, ui, settings, &dirs.settings_path);
    commit::setup_setting_commit(
        &settings_win,
        ui,
        settings,
        &dirs.settings_path,
        dirs.favicons_dir.clone(),
        dirs.thumbnails_dir.clone(),
        db,
        hotkey_tx,
        retention_tx,
        blacklist_state,
        image_preview_size,
        theme_fillers,
    );
    // Registered here, not at startup: both only forward actions from this
    // window, which cannot exist before it does.
    crate::stats_ui::setup_stats(&settings_win, db, &dirs.db_path);
    crate::maintenance::setup_manual_maintenance(ui, db, dirs, settings, &settings_win);

    settings_win
}
