use slint::ComponentHandle;

fn idx_of(needle: &str, haystack: &[&str]) -> i32 {
    haystack
        .iter()
        .position(|&s| s.eq_ignore_ascii_case(needle))
        .unwrap_or(0) as i32
}

/// Derive a display name from a bare path (the file name without extension),
/// falling back to the whole path when there is no stem.
fn derive_app_name(path: &str) -> String {
    std::path::Path::new(path)
        .file_stem()
        .and_then(|s| s.to_str())
        .filter(|s| !s.is_empty())
        .unwrap_or(path)
        .to_string()
}

/// Parse a comma-separated list of `Name: path` entries into `SendToApp`
/// structs. A bare path (no colon) derives its name from the file name.
fn parse_send_to_apps(raw: &str) -> Vec<cliptoo_core::SendToApp> {
    raw.split(',')
        .map(|entry| entry.trim())
        .filter(|entry| !entry.is_empty())
        .map(|entry| match entry.split_once(':') {
            Some((name, path)) => {
                let name = name.trim();
                let path = path.trim();
                cliptoo_core::SendToApp {
                    name: if name.is_empty() {
                        derive_app_name(path)
                    } else {
                        name.to_string()
                    },
                    path: path.to_string(),
                }
            }
            None => cliptoo_core::SendToApp {
                name: derive_app_name(entry),
                path: entry.to_string(),
            },
        })
        .collect()
}

/// Render `SendToApp`s back into the comma-separated `Name: path` form the
/// settings text field shows.
fn format_send_to_apps(apps: &[cliptoo_core::SendToApp]) -> String {
    apps.iter()
        .map(|a| format!("{}: {}", a.name, a.path))
        .collect::<Vec<_>>()
        .join(", ")
}

/// Parse a comma-separated list of app identifiers.
fn parse_blacklist(raw: &str) -> Vec<String> {
    raw.split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(ToOwned::to_owned)
        .collect()
}

/// Derive the custom accent hex from the three tuning sliders: HSV hue in
/// degrees (0–360) plus saturation and brightness (both 0.0–1.0).
fn accent_hex(hue: f64, saturation: f64, value: f64) -> String {
    let (r, g, b) = crate::theme::hsv_to_rgb(hue, saturation, value);
    format!("#{r:02X}{g:02X}{b:02X}")
}

/// The HSV of the accent currently in effect. A custom hex is authoritative
/// (a color chosen before the tuning sliders existed may not match the
/// persisted tuning values); with no custom color ("Clear" / OS accent) the
/// detected OS accent is used, falling back to the persisted tuning values.
/// Returning all three components means the sliders always start from the
/// accent that is actually shown, so tuning one of them (e.g. brightness on a
/// muted OS green) never makes the other components jump to stored defaults.
fn current_accent_hsv(
    s: &cliptoo_core::Settings,
    system_accent: Option<(u8, u8, u8)>,
) -> (f64, f64, f64) {
    if !s.accent_color.trim().is_empty() {
        let (r, g, b) = crate::theme::parse_accent_hex(&s.accent_color);
        crate::theme::rgb_to_hsv(r, g, b)
    } else if let Some((r, g, b)) = system_accent {
        crate::theme::rgb_to_hsv(r, g, b)
    } else {
        (s.accent_hue, s.accent_saturation, s.accent_value)
    }
}

/// Apply one slider move to the currently shown tuning. The moved component
/// (`key` in {"accent_hue", "accent_saturation", "accent_value"}) is set to
/// `value`; the other two stay locked at the values currently displayed. Hue
/// arrives in degrees 0–360, saturation/brightness in percent. Baselining on
/// the displayed values (not on a re-parse of the derived hex) is what keeps
/// the untouched sliders from wobbling.
fn retune_accent(
    current_hue: f64,
    current_saturation: f64,
    current_value: f64,
    key: &str,
    value: f64,
) -> (f64, f64, f64) {
    match key {
        "accent_hue" => (value.clamp(0.0, 360.0), current_saturation, current_value),
        "accent_saturation" => (current_hue, value.clamp(0.0, 100.0) / 100.0, current_value),
        _ => (
            current_hue,
            current_saturation,
            value.clamp(0.0, 100.0) / 100.0,
        ),
    }
}

/// Instant-filter keyword lists, one per setting row, grouped by section.
/// The header search box is matched against these (case-insensitive) by
/// `apply_settings_filter`; a row shows when the query matches any keyword.
/// Slint has no substring matching, so this lives on the Rust side.
const GENERAL_HOTKEY: &str = "general launch hotkey toggle shortcut global";
const GENERAL_STARTUP: &str = "general start with system autostart login";
const GENERAL_TRAY: &str = "general always close to tray background hide tray";
const GENERAL_QUICKPASTE: &str = "general quick paste modifier right alt left alt control";
const GENERAL_PLAINTEXT: &str = "general paste as plain text formatting strip";
const GENERAL_PASTE_TOP: &str = "general paste moves clip to top bump reorder";
const GENERAL_LOGLEVEL: &str = "general log level logging verbosity debug info warn error";
const GENERAL_LOGFILE: &str = "general log file open latest log viewer";
const APPEARANCE_THEME: &str = "appearance theme system light dark mode";
const APPEARANCE_ACCENT: &str = "appearance accent color swatch clear picker";
const APPEARANCE_ACCENT_HUE: &str = "appearance accent hue color wheel degrees";
const APPEARANCE_ACCENT_SAT: &str = "appearance accent saturation intensity color vivid";
const APPEARANCE_ACCENT_VALUE: &str = "appearance accent brightness value light dark";
const APPEARANCE_FONT: &str = "appearance font family typeface picker";
const APPEARANCE_CLIP_FONT_SIZE: &str = "appearance clip list font size text";
const APPEARANCE_PREVIEW_FONT_SIZE: &str = "appearance preview font size code color";
const APPEARANCE_PADDING: &str = "appearance row padding compact standard luxury";
const APPEARANCE_HOVER_DELAY: &str = "appearance preview hover delay tooltip milliseconds";
const APPEARANCE_IMAGE_PREVIEW_SIZE: &str = "appearance image preview size thumbnail pixels";
const EXTERNAL_DIFF_TOOL: &str = "external apps diff tool path compare";
const EXTERNAL_SENDTO: &str = "external apps send to apps list";
const EXTERNAL_BLACKLIST: &str = "external apps blacklist apps exclude ignore";

/// `true` when `query` (already lowercased and trimmed) matches `keywords`.
fn row_matches(keywords: &str, query: &str) -> bool {
    query.is_empty() || keywords.contains(query)
}

/// Apply the settings-page filter to every option row. Called on each search
/// keystroke (from the `filter-changed` callback) and when the window opens
/// (with an empty query, which shows everything).
fn apply_settings_filter(win: &crate::SettingsWindow, query: &str) {
    let q = query.trim().to_lowercase();
    win.set_row_hotkey_visible(row_matches(GENERAL_HOTKEY, &q));
    win.set_row_startup_visible(row_matches(GENERAL_STARTUP, &q));
    win.set_row_tray_visible(row_matches(GENERAL_TRAY, &q));
    win.set_row_quickpaste_visible(row_matches(GENERAL_QUICKPASTE, &q));
    win.set_row_plaintext_visible(row_matches(GENERAL_PLAINTEXT, &q));
    win.set_row_paste_top_visible(row_matches(GENERAL_PASTE_TOP, &q));
    win.set_row_loglevel_visible(row_matches(GENERAL_LOGLEVEL, &q));
    win.set_row_logfile_visible(row_matches(GENERAL_LOGFILE, &q));
    win.set_row_theme_visible(row_matches(APPEARANCE_THEME, &q));
    win.set_row_accent_visible(row_matches(APPEARANCE_ACCENT, &q));
    win.set_row_accent_hue_visible(row_matches(APPEARANCE_ACCENT_HUE, &q));
    win.set_row_accent_sat_visible(row_matches(APPEARANCE_ACCENT_SAT, &q));
    win.set_row_accent_value_visible(row_matches(APPEARANCE_ACCENT_VALUE, &q));
    win.set_row_font_visible(row_matches(APPEARANCE_FONT, &q));
    win.set_row_clip_font_size_visible(row_matches(APPEARANCE_CLIP_FONT_SIZE, &q));
    win.set_row_preview_font_size_visible(row_matches(APPEARANCE_PREVIEW_FONT_SIZE, &q));
    win.set_row_padding_visible(row_matches(APPEARANCE_PADDING, &q));
    win.set_row_hover_delay_visible(row_matches(APPEARANCE_HOVER_DELAY, &q));
    win.set_row_image_preview_size_visible(row_matches(APPEARANCE_IMAGE_PREVIEW_SIZE, &q));
    win.set_row_diff_tool_visible(row_matches(EXTERNAL_DIFF_TOOL, &q));
    win.set_row_sendto_visible(row_matches(EXTERNAL_SENDTO, &q));
    win.set_row_blacklist_visible(row_matches(EXTERNAL_BLACKLIST, &q));
}

/// Persist the settings window's current size into `Settings` so a resized
/// window keeps its size across restarts, and update the window's own
/// `stored-width`/`stored-height` so its `preferred-*` binding reflects the
/// resized geometry (otherwise re-showing the window snaps back to the stale
/// preferred size). Called on both close paths.
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
    s.settings_window_height = size.height as f64;
    let _ = s.save(path);
}

/// Normalise a hotkey string captured from the settings UI. Control
/// characters (a letter pressed with a modifier arrives as U+0001..U+001A)
/// are mapped to letters, and the final key token is uppercased so the
/// assigned key always displays uppercase (e.g. `Ctrl+Alt+q` → `Ctrl+Alt+Q`).
///
/// A trailing `+` is the plus key itself (`Ctrl++` = Ctrl + the plus key)
/// when a `+` precedes it; a lone trailing `+` is a dangling separator from
/// capturing a modifier without a key and is dropped.
fn clean_hotkey_text(raw: &str) -> String {
    let cleaned: String = raw
        .chars()
        .map(|c| {
            let code = c as u32;
            if (1..=26).contains(&code) {
                ((b'a' + (code - 1) as u8) as char).to_string()
            } else {
                c.to_string()
            }
        })
        .collect();
    match cleaned.strip_suffix('+') {
        // `before` ends with `+`, so the string was `<mods>+` + the `+` key.
        Some(before) if before.ends_with('+') => {
            let mut out = before.trim_end_matches('+').to_string();
            out.push_str("++");
            out
        }
        // A lone `+` is the bare plus key itself.
        Some("") => cleaned.to_uppercase(),
        // The trailing `+` was a separator with no key after it.
        Some(_) => cleaned.trim_end_matches('+').to_uppercase(),
        None => match cleaned.rsplit_once('+') {
            Some((mods, key)) if !key.is_empty() => format!("{mods}+{}", key.to_uppercase()),
            _ => cleaned.to_uppercase(),
        },
    }
}

/// Update a single `Theme` token on both the main window and the settings
/// window globals. Slint globals are per-window-instance, so live previews
/// need each window's `Theme` global updated.
fn apply_theme_to_windows(
    main_ui: &slint::Weak<crate::AppWindow>,
    settings_win_ui: &slint::Weak<crate::SettingsWindow>,
    apply: impl Fn(&crate::Theme),
) {
    if let Some(ui) = main_ui.upgrade() {
        apply(&ui.global::<crate::Theme>());
    }
    if let Some(win) = settings_win_ui.upgrade() {
        apply(&win.global::<crate::Theme>());
    }
}

fn reapply_theme(
    main_ui: &slint::Weak<crate::AppWindow>,
    settings_win_ui: &slint::Weak<crate::SettingsWindow>,
    settings: &cliptoo_core::Settings,
    favicons_dir: std::path::PathBuf,
) {
    let main_weak = main_ui.clone();
    let settings_weak = settings_win_ui.clone();
    let s_snap = settings.clone();
    tokio::spawn(async move {
        let prev_dark = crate::theme::cached_resolved_theme().0;
        let (is_dark, system_accent) = crate::theme::resolve_theme(&s_snap).await;
        let _ = main_weak.upgrade_in_event_loop(move |ui| {
            crate::theme::fill_theme(
                &ui.global::<crate::Theme>(),
                &s_snap,
                is_dark,
                system_accent,
            );
            if is_dark != prev_dark {
                // The visible list rows cache decoded favicon images keyed by
                // theme variant, so a light→dark (or vice-versa) switch must
                // reload them to avoid showing an invisible icon.
                crate::thumbnail_cache::reload_favicons(&ui, &favicons_dir);
            }
            if let Some(win) = settings_weak.upgrade() {
                crate::theme::fill_theme(
                    &win.global::<crate::Theme>(),
                    &s_snap,
                    is_dark,
                    system_accent,
                );
            }
        });
    });
}

/// Reset settings-open and persist the window size when the settings window
/// is closed via the window manager (ESC is handled by the settings-closing
/// callback).
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
        if let Some(win) = sw.upgrade() {
            persist_window_size(&win, &s, &p);
        }
        if let Some(ui) = main_ui.upgrade() {
            ui.set_settings_open(false);
        }
        slint::CloseRequestResponse::HideWindow
    });
}

/// Push every persisted setting into the window's properties.
fn init_settings_properties(
    settings_win: &crate::SettingsWindow,
    settings: &std::rc::Rc<std::cell::RefCell<cliptoo_core::Settings>>,
) {
    let s = settings.borrow();
    settings_win.set_stored_width(s.settings_window_width as f32);
    settings_win.set_stored_height(s.settings_window_height as f32);
    settings_win.set_s_hotkey(clean_hotkey_text(s.hotkey.as_str()).into());
    settings_win.set_s_start_with_system(s.start_with_system);
    settings_win.set_s_always_close_to_tray(s.always_close_to_tray);
    settings_win.set_s_quick_paste_mod_idx(idx_of(
        &s.quick_paste_modifier,
        &["Right Alt", "Left Alt", "Control"],
    ));
    settings_win.set_s_log_level_idx(idx_of(
        &s.logging_level,
        &["Debug", "Info", "Warn", "Error"],
    ));
    settings_win.set_s_theme_idx(idx_of(&s.theme, &["System", "Light", "Dark"]));
    settings_win.set_s_accent_color(if s.accent_color.trim().is_empty() {
        crate::theme::default_accent_color()
    } else {
        crate::theme::accent_hex_to_color(&s.accent_color)
    });
    let (accent_h, accent_s, accent_v) =
        current_accent_hsv(&s, crate::theme::cached_resolved_theme().1);
    settings_win.set_s_accent_hue(accent_h.round() as i32);
    settings_win.set_s_accent_saturation((accent_s * 100.0).round() as i32);
    settings_win.set_s_accent_value((accent_v * 100.0).round() as i32);
    settings_win.set_s_font_family(s.font_family.as_str().into());
    settings_win.set_s_font_size_hundredths((s.font_size * 100.0) as i32);
    settings_win.set_s_preview_font_size_hundredths((s.preview_font_size * 100.0) as i32);
    settings_win.set_s_row_padding_idx(idx_of(
        &s.clip_item_padding,
        &["Compact", "Standard", "Luxury"],
    ));
    settings_win.set_hover_delay(s.hover_preview_delay as i32);
    settings_win.set_s_image_preview_size(s.hover_image_preview_size as i32);
    settings_win.set_s_paste_as_plain_text(s.paste_as_plain_text);
    settings_win.set_s_paste_moves_to_top(s.paste_moves_clip_to_top);
    settings_win.set_s_diff_tool_path(s.compare_tool_path.as_str().into());
    settings_win.set_s_send_to_apps(format_send_to_apps(&s.send_to_apps).into());
    settings_win.set_s_blacklist_apps(s.blacklisted_apps.join(", ").into());
    settings_win.set_s_max_clips(s.max_clips as i32);
    settings_win.set_s_max_age_days(s.max_age_days as i32);
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

/// Instant filter search: the header query is matched against each option
/// row's keywords and drives the row visibility. Runs on the UI thread (the
/// callback fires from Slint's edited handler), so setters are safe here.
fn setup_settings_filter(settings_win: &crate::SettingsWindow) {
    let sw = settings_win.as_weak();
    settings_win.on_filter_changed(move |query: slint::SharedString| {
        if let Some(win) = sw.upgrade() {
            apply_settings_filter(&win, &query);
        }
    });
}

/// Clear accent: empty accent_color means "use the OS default accent".
fn setup_clear_accent(
    settings_win: &crate::SettingsWindow,
    main_ui: &crate::AppWindow,
    settings: &std::rc::Rc<std::cell::RefCell<cliptoo_core::Settings>>,
    settings_path: &std::path::Path,
) {
    let sw = settings_win.as_weak();
    let settings_ui = main_ui.as_weak();
    let s = settings.clone();
    let p = settings_path.to_path_buf();
    settings_win.on_clear_accent_color(move || {
        // Clear synchronously: Rc<RefCell<Settings>> is !Send, so it must
        // not cross into the spawned task. Snapshot the cleared settings.
        let s_snap;
        {
            let mut s = s.borrow_mut();
            s.accent_color = String::new();
            s_snap = s.clone();
        }
        let sw = sw.clone();
        let settings_ui = settings_ui.clone();
        let p = p.clone();
        tokio::spawn(async move {
            // Resolve first so the shared cache holds the freshly detected
            // OS accent; the settings swatch reads that cache. Without this,
            // a previously custom accent leaves the cache stale and "Clear"
            // shows the fallback color until a second click.
            let (is_dark, system_accent) = crate::theme::resolve_theme(&s_snap).await;
            let swatch = crate::theme::default_accent_color();
            // Seed the tuning sliders from the OS accent so they reflect
            // the swatch instead of the stale custom tuning values; the
            // persisted fields are refreshed on the first slider move.
            let os_hsv = system_accent.map(|(r, g, b)| crate::theme::rgb_to_hsv(r, g, b));
            let main_weak = settings_ui.clone();
            let settings_weak = sw.clone();
            let s_main = s_snap.clone();
            let s_settings = s_snap.clone();
            let _ = main_weak.upgrade_in_event_loop(move |ui| {
                crate::theme::fill_theme(
                    &ui.global::<crate::Theme>(),
                    &s_main,
                    is_dark,
                    system_accent,
                );
            });
            let _ = settings_weak.upgrade_in_event_loop(move |win| {
                crate::theme::fill_theme(
                    &win.global::<crate::Theme>(),
                    &s_settings,
                    is_dark,
                    system_accent,
                );
                win.set_s_accent_color(swatch);
                if let Some((h, sat, val)) = os_hsv {
                    win.set_s_accent_hue(h.round() as i32);
                    win.set_s_accent_saturation((sat * 100.0).round() as i32);
                    win.set_s_accent_value((val * 100.0).round() as i32);
                }
            });
            let _ = s_snap.save(&p);
        });
    });
}

/// Open the latest log file via the system default viewer.
fn setup_open_log(settings_win: &crate::SettingsWindow, logs_dir: &std::path::Path) {
    let logs_dir = logs_dir.to_path_buf();
    settings_win.on_open_log(move || {
        let Some(latest) = cliptoo_core::logger::latest_log_path(&logs_dir) else {
            tracing::warn!("open-log: no log file yet in {}", logs_dir.display());
            return;
        };
        if let Err(e) = std::process::Command::new("xdg-open").arg(&latest).spawn() {
            tracing::warn!(
                "open-log: failed to launch xdg-open for {}: {e}",
                latest.display()
            );
        }
    });
}

/// When the settings window closes, reset settings-open so ESC and blur-to-tray
/// work again on the main window, and persist the size.
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
        if let Some(win) = sw.upgrade() {
            persist_window_size(&win, &s, &p);
        }
        if let Some(ui) = main_ui.upgrade() {
            ui.set_settings_open(false);
        }
    });
}

/// Font picker — native KDE font dialog via Qt (PyQt6).
fn setup_font_picker(
    settings_win: &crate::SettingsWindow,
    main_ui: &crate::AppWindow,
    settings: &std::rc::Rc<std::cell::RefCell<cliptoo_core::Settings>>,
    settings_path: &std::path::Path,
) {
    let sw = settings_win.as_weak();
    let settings_ui = main_ui.as_weak();
    let s = settings.clone();
    let p = settings_path.to_path_buf();
    settings_win.on_font_picker(move || {
        let script = r#"
from PyQt6.QtWidgets import QApplication, QFontDialog
app = QApplication([])
font, ok = QFontDialog.getFont()
if ok:
    print(font.family())
"#;
        let Ok(output) = std::process::Command::new("python3")
            .arg("-c")
            .arg(script)
            .output()
        else {
            return;
        };
        let family = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if family.is_empty() {
            return;
        }
        let mut s = s.borrow_mut();
        s.font_family.clone_from(&family);
        if let Some(win) = sw.upgrade() {
            win.set_s_font_family(family.as_str().into());
        }
        apply_theme_to_windows(&settings_ui, &sw, |t| {
            t.set_font_family(family.as_str().into());
        });
        let _ = s.save(&p);
    });
}

/// Handle setting changes: persist each key/value into `Settings` and re-apply
/// live effects (theme, fonts, hotkeys).
#[allow(clippy::too_many_arguments)]
fn setup_setting_commit(
    settings_win: &crate::SettingsWindow,
    main_ui: &crate::AppWindow,
    settings: &std::rc::Rc<std::cell::RefCell<cliptoo_core::Settings>>,
    settings_path: &std::path::Path,
    favicons_dir: std::path::PathBuf,
    hotkey_tx: tokio::sync::watch::Sender<String>,
    retention_tx: tokio::sync::watch::Sender<cliptoo_core::maintenance::RetentionConfig>,
    blacklist_state: std::sync::Arc<std::sync::Mutex<Vec<String>>>,
) {
    let s = settings.clone();
    let p = settings_path.to_path_buf();
    let settings_ui = main_ui.as_weak();
    let sw = settings_win.as_weak();
    settings_win.on_setting_changed(
        move |key: slint::SharedString, value: slint::SharedString| {
            let key = key.to_string();
            let value = value.to_string();
            let mut s = s.borrow_mut();

            match key.as_str() {
                "hotkey" => {
                    // No trim: clean_hotkey_text keeps a trailing `+` as the
                    // plus key (Ctrl++ = Ctrl + the plus key) and drops a
                    // dangling separator itself.
                    let cleaned = clean_hotkey_text(&value);
                    s.hotkey = cleaned.clone();
                    if let Some(win) = sw.upgrade() {
                        win.set_s_hotkey(cleaned.into());
                    }
                    let _ = hotkey_tx.send(s.hotkey.clone());
                }
                "start_with_system" => {
                    let enabled = value == "true";
                    s.start_with_system = enabled;
                    if enabled {
                        let _ = crate::autostart::ensure_autostart();
                    } else {
                        let _ = crate::autostart::remove_autostart();
                    }
                }
                "always_close_to_tray" => s.always_close_to_tray = value == "true",
                "quick_paste_modifier" => {
                    s.quick_paste_modifier = value.clone();
                    if let Some(ui) = settings_ui.upgrade() {
                        ui.set_quick_paste_mod(value.clone().into());
                    }
                }
                "logging_level" => {
                    s.logging_level = value.clone();
                    // Apply live: the file logger reads the level from an
                    // atomic, so the new level is in effect immediately.
                    cliptoo_core::logger::set_level(s.log_level_filter());
                }
                "theme" => {
                    s.theme = value.clone();
                    reapply_theme(&settings_ui, &sw, &s, favicons_dir.clone());
                }
                "font_family" => {
                    s.font_family = value.clone();
                    apply_theme_to_windows(&settings_ui, &sw, |t| {
                        t.set_font_family(value.as_str().into());
                    });
                }
                "accent_color" => {
                    let hex = value.trim();
                    if hex.starts_with('#') && hex.len() == 7 {
                        s.accent_color = value.clone();
                        if let Some(win) = sw.upgrade() {
                            win.set_s_accent_color(crate::theme::accent_hex_to_color(hex));
                        }
                        reapply_theme(&settings_ui, &sw, &s, favicons_dir.clone());
                    }
                }
                "accent_hue" | "accent_saturation" | "accent_value" => {
                    if let Ok(v) = value.parse::<f64>() {
                        // Baseline from the sliders as currently shown: the
                        // full HSV of the accent in effect (a custom hex,
                        // the OS accent while "Clear" is active, or the
                        // persisted tuning) was folded into them when the
                        // window opened or "Clear" ran. Only the moved
                        // slider changes — the others stay locked at their
                        // displayed values. Re-deriving the baseline from
                        // `s.accent_color` (a hex round-trip) let the
                        // untouched sliders wobble by ±1 unit per move.
                        let (is_dark, _) = crate::theme::cached_resolved_theme();
                        let (h, sat, val) = if let Some(win) = sw.upgrade() {
                            retune_accent(
                                win.get_s_accent_hue() as f64,
                                win.get_s_accent_saturation() as f64 / 100.0,
                                win.get_s_accent_value() as f64 / 100.0,
                                &key,
                                v,
                            )
                        } else {
                            (s.accent_hue, s.accent_saturation, s.accent_value)
                        };
                        // Moving any slider defines a custom accent, so this
                        // also works from a "Clear" (OS accent) start
                        // instead of doing nothing until a color is picked.
                        s.accent_hue = h;
                        s.accent_saturation = sat;
                        s.accent_value = val;
                        s.accent_color = accent_hex(h, sat, val);
                        if let Some(ui) = settings_ui.upgrade() {
                            crate::theme::fill_theme(
                                &ui.global::<crate::Theme>(),
                                &s,
                                is_dark,
                                None,
                            );
                        }
                        if let Some(win) = sw.upgrade() {
                            win.set_s_accent_hue(h.round() as i32);
                            win.set_s_accent_saturation((sat * 100.0).round() as i32);
                            win.set_s_accent_value((val * 100.0).round() as i32);
                            win.set_s_accent_color(crate::theme::accent_hex_to_color(
                                &s.accent_color,
                            ));
                            crate::theme::fill_theme(
                                &win.global::<crate::Theme>(),
                                &s,
                                is_dark,
                                None,
                            );
                        }
                    }
                }
                "font_size" => {
                    if let Ok(v) = value.parse::<f64>() {
                        s.font_size = v;
                        apply_theme_to_windows(&settings_ui, &sw, |t| {
                            t.set_clip_list_font_size(v as f32)
                        });
                    }
                }
                "preview_font_size" => {
                    if let Ok(v) = value.parse::<f64>() {
                        s.preview_font_size = v;
                        apply_theme_to_windows(&settings_ui, &sw, |t| {
                            t.set_preview_font_size(v as f32);
                        });
                    }
                }
                "clip_item_padding" => {
                    s.clip_item_padding = value.clone();
                    apply_theme_to_windows(&settings_ui, &sw, |t| {
                        t.set_row_height(crate::positioning::row_height(value.as_str()) as f32);
                    });
                }
                "hover_preview_delay" => {
                    if let Ok(ms) = value.parse::<u32>() {
                        s.hover_preview_delay = ms;
                        apply_theme_to_windows(&settings_ui, &sw, |t| {
                            t.set_hover_preview_delay(ms as i64);
                        });
                    }
                }
                "hover_image_preview_size" => {
                    if let Ok(v) = value.parse::<u32>() {
                        s.hover_image_preview_size = v;
                    }
                }
                "paste_as_plain_text" => s.paste_as_plain_text = value == "true",
                "paste_moves_to_top" => s.paste_moves_clip_to_top = value == "true",
                "compare_tool_path" => s.compare_tool_path = value.clone(),
                "send_to_apps" => {
                    s.send_to_apps = parse_send_to_apps(&value);
                    // Rebuild the context-menu Send To list so the change
                    // applies without a restart.
                    if let Some(ui) = settings_ui.upgrade() {
                        let names: Vec<slint::SharedString> = s
                            .send_to_apps
                            .iter()
                            .map(|a| slint::SharedString::from(a.name.as_str()))
                            .collect();
                        ui.set_ctx_send_to_apps(
                            std::rc::Rc::new(slint::VecModel::from(names)).into(),
                        );
                    }
                }
                "blacklisted_apps" => {
                    s.blacklisted_apps = parse_blacklist(&value);
                    // The clipboard listener reads the blacklist from this
                    // shared state on every poll, so the change applies
                    // without a restart.
                    *blacklist_state
                        .lock()
                        .unwrap_or_else(|poisoned| poisoned.into_inner()) =
                        s.blacklisted_apps.clone();
                }
                "max_clips" => {
                    if let Ok(v) = value.parse::<u32>() {
                        s.max_clips = v;
                    }
                }
                "max_age_days" => {
                    if let Ok(v) = value.parse::<u32>() {
                        s.max_age_days = v;
                    }
                }
                _ => {}
            }

            let _ = s.save(&p);

            // Publish retention changes so the scheduled maintenance task picks
            // them up on its next pass without a restart.
            if matches!(key.as_str(), "max_clips" | "max_age_days") {
                let _ = retention_tx.send(cliptoo_core::maintenance::RetentionConfig {
                    max_clips: s.max_clips,
                    max_age_days: s.max_age_days,
                });
            }
        },
    );
}

/// Show the settings window from the hamburger menu.
fn setup_menu_open(settings_win: &crate::SettingsWindow, main_ui: &crate::AppWindow) {
    let sw = settings_win.as_weak();
    let weak_ui = main_ui.as_weak();
    main_ui.on_menu_settings(move || {
        if let Some(win) = sw.upgrade() {
            if let Some(ui) = weak_ui.upgrade() {
                // Guard blur-to-tray while the settings window is visible;
                // cleared when the settings window closes.
                ui.set_settings_open(true);
            }
            // Always reopen on the settings page (not the Database page),
            // with a cleared filter, regardless of where the user left off.
            win.set_on_database_page(false);
            win.set_settings_filter("".into());
            apply_settings_filter(&win, "");
            // preferred-height is shrunk by the WM decorations on Wayland, so
            // size the window explicitly to open at the computed content size.
            win.window().set_size(slint::LogicalSize {
                width: win.get_stored_width(),
                height: win.get_desired_height(),
            });
            win.show().ok();
            win.invoke_focus_search();
            // Qt's show() doesn't raise a freshly-shown window; after a popup
            // (context/hamburger menu) closes it can restart under the main
            // window. Raise+activate so Settings always opens on top.
            crate::drag::activate_window(&win);
        }
    });
}

pub fn setup_settings_window(
    ui: &crate::AppWindow,
    settings: &std::rc::Rc<std::cell::RefCell<cliptoo_core::Settings>>,
    dirs: &crate::app_dirs::AppDirs,
    hotkey_tx: tokio::sync::watch::Sender<String>,
    retention_tx: tokio::sync::watch::Sender<cliptoo_core::maintenance::RetentionConfig>,
    blacklist_state: std::sync::Arc<std::sync::Mutex<Vec<String>>>,
) -> crate::SettingsWindow {
    let settings_win = crate::SettingsWindow::new().expect("SettingsWindow creation");

    setup_close_persistence(&settings_win, ui, settings, &dirs.settings_path);
    init_settings_properties(&settings_win, settings);
    setup_maintenance_forwarding(&settings_win, ui);
    setup_settings_filter(&settings_win);
    setup_clear_accent(&settings_win, ui, settings, &dirs.settings_path);
    setup_open_log(&settings_win, &dirs.logs_dir);
    setup_settings_closing(&settings_win, ui, settings, &dirs.settings_path);
    setup_font_picker(&settings_win, ui, settings, &dirs.settings_path);
    setup_setting_commit(
        &settings_win,
        ui,
        settings,
        &dirs.settings_path,
        dirs.favicons_dir.clone(),
        hotkey_tx,
        retention_tx,
        blacklist_state,
    );
    setup_menu_open(&settings_win, ui);

    settings_win
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A saturation/brightness change must keep the hue read back from the
    /// current custom hex (no swatch-grid snapping anymore), and the result
    /// must itself be a valid accent at the new saturation/brightness.
    #[test]
    fn accent_hex_keeps_hue_when_retuned() {
        let (s_old, v_old) = (0.9, 0.95);
        let (s_new, v_new) = (0.45, 0.75);
        for hue in [0.0, 30.0, 120.0, 180.0, 247.0, 300.0] {
            let hex = accent_hex(hue, s_old, v_old);
            let (r, g, b) = crate::theme::parse_accent_hex(&hex);
            let recovered = crate::theme::rgb_to_hsv(r, g, b).0;
            let retuned = accent_hex(recovered, s_new, v_new);
            let (rr, rg, rb) = crate::theme::parse_accent_hex(&retuned);
            let (h, s, v) = crate::theme::rgb_to_hsv(rr, rg, rb);
            assert!((h - recovered).abs() < 1.5, "hue drifted {hue}: {h}");
            assert!((s - s_new).abs() < 0.02, "saturation drifted {hue}: {s}");
            assert!((v - v_new).abs() < 0.02, "brightness drifted {hue}: {v}");
        }
    }

    /// Tuning from a "Clear" (OS accent) start must start from the OS accent's
    /// hue, saturation and brightness — not the stored tuning defaults — so a
    /// muted green stays green and muted when its brightness is adjusted.
    #[test]
    fn current_accent_hsv_uses_os_accent_when_clear() {
        let s = cliptoo_core::Settings::default();
        let (r, g, b) = crate::theme::hsv_to_rgb(120.0, 0.3, 0.5);
        let (h, sat, val) = current_accent_hsv(&s, Some((r, g, b)));
        assert!((h - 120.0).abs() < 2.0, "OS hue drifted: {h}");
        assert!((sat - 0.3).abs() < 0.02, "OS saturation drifted: {sat}");
        assert!((val - 0.5).abs() < 0.02, "OS brightness drifted: {val}");
    }

    /// A custom hex is authoritative for all three tuning components.
    #[test]
    fn current_accent_hsv_reads_custom_hex() {
        let s = cliptoo_core::Settings {
            accent_color: "#7C6EE6".into(),
            ..cliptoo_core::Settings::default()
        };
        let (r, g, b) = crate::theme::parse_accent_hex(&s.accent_color);
        let (eh, es, ev) = crate::theme::rgb_to_hsv(r, g, b);
        let (h, sat, val) = current_accent_hsv(&s, None);
        assert!((h - eh).abs() < f64::EPSILON);
        assert!((sat - es).abs() < f64::EPSILON);
        assert!((val - ev).abs() < f64::EPSILON);
    }

    /// Moving one tuning slider leaves the other two exactly at their current
    /// values — bit-identical, not a hex round-trip — so repeated moves never
    /// make the untouched sliders drift.
    #[test]
    fn retune_accent_locks_untouched_sliders() {
        let (h, sat, val) = retune_accent(247.0, 0.65, 0.95, "accent_value", 72.0);
        assert_eq!(h, 247.0);
        assert_eq!(sat, 0.65);
        assert_eq!(val, 0.72);

        let (h, sat, val) = retune_accent(247.0, 0.65, 0.95, "accent_saturation", 30.0);
        assert_eq!(h, 247.0);
        assert_eq!(sat, 0.30);
        assert_eq!(val, 0.95);

        let (h, sat, val) = retune_accent(247.0, 0.65, 0.95, "accent_hue", 120.0);
        assert_eq!(h, 120.0);
        assert_eq!(sat, 0.65);
        assert_eq!(val, 0.95);
    }

    /// Out-of-range slider input is clamped to the slider's bounds.
    #[test]
    fn retune_accent_clamps_input() {
        let (h, sat, val) = retune_accent(247.0, 0.65, 0.95, "accent_value", 500.0);
        assert_eq!(h, 247.0);
        assert_eq!(sat, 0.65);
        assert_eq!(val, 1.0);

        let (h, sat, val) = retune_accent(247.0, 0.65, 0.95, "accent_hue", -40.0);
        assert_eq!(h, 0.0);
        assert_eq!(sat, 0.65);
        assert_eq!(val, 0.95);
    }

    #[test]
    fn parse_send_to_apps_handles_name_path_and_bare_paths() {
        let apps = parse_send_to_apps("code: /usr/bin/code, gedit");
        assert_eq!(apps.len(), 2);
        assert_eq!(apps[0].name, "code");
        assert_eq!(apps[0].path, "/usr/bin/code");
        assert_eq!(apps[1].name, "gedit");
        assert_eq!(apps[1].path, "gedit");

        // Empty entries and whitespace are dropped; a missing name is derived
        // from the path's file stem.
        let apps = parse_send_to_apps("  ,, /opt/tools/meld, : /usr/bin/kompare ,");
        assert_eq!(apps.len(), 2);
        assert_eq!(apps[0].name, "meld");
        assert_eq!(apps[1].name, "kompare");
        assert_eq!(apps[1].path, "/usr/bin/kompare");
    }

    #[test]
    fn send_to_apps_round_trip_format_and_parse() {
        let apps = parse_send_to_apps("code: /usr/bin/code, gedit");
        let formatted = format_send_to_apps(&apps);
        assert_eq!(formatted, "code: /usr/bin/code, gedit: gedit");
        assert_eq!(parse_send_to_apps(&formatted).len(), 2);
    }

    #[test]
    fn parse_blacklist_trims_and_drops_empty() {
        assert_eq!(parse_blacklist(""), Vec::<String>::new());
        assert_eq!(
            parse_blacklist("  ,, org.kde.dolphin ,, kwrite "),
            vec!["org.kde.dolphin".to_string(), "kwrite".to_string()]
        );
    }
}
