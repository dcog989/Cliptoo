use slint::ComponentHandle;

/// Suffixes of the cached hover-preview images written by `cliptoo_core::image`
/// (see `store_both_thumbnails_for_file`). Used to invalidate them when the
/// preview size setting changes.
const PREVIEW_WEBP_SUFFIX: &str = "_preview.webp";
const PREVIEW_SVG_SUFFIX: &str = "_preview.svg";

/// Index of `needle` in `haystack` (case-insensitive), for combo boxes whose
/// persisted value came from the same option list. Falls back to index 0 when
/// `needle` is not in the list (a hand-edit or a value removed between
/// versions) but logs a warning so the mismatch is visible instead of silently
/// showing the first option.
fn idx_of(needle: &str, haystack: &[&str]) -> i32 {
    match haystack
        .iter()
        .position(|&s| s.eq_ignore_ascii_case(needle))
    {
        Some(i) => i as i32,
        None => {
            tracing::warn!(
                "settings: '{needle}' is not one of the valid values ({haystack:?}); using the default"
            );
            0
        }
    }
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
/// settings text field shows. An entry whose name was derived from its path
/// (a bare path like `gedit` parses to name == path) stays bare so a
/// format+parse round-trip preserves the user's original form instead of
/// rewriting `gedit` into `gedit: gedit`.
fn format_send_to_apps(apps: &[cliptoo_core::SendToApp]) -> String {
    apps.iter()
        .map(|a| {
            if a.name == a.path {
                a.path.clone()
            } else {
                format!("{}: {}", a.name, a.path)
            }
        })
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
const APPEARANCE_FONT: &str = "appearance font family typeface picker";
const APPEARANCE_CLIP_FONT_SIZE: &str = "appearance clip list font size text";
const APPEARANCE_PREVIEW_FONT_SIZE: &str = "appearance preview font size code color";
const APPEARANCE_PADDING: &str = "appearance row padding compact standard luxury";
const APPEARANCE_HOVER_DELAY: &str = "appearance preview hover delay tooltip milliseconds";
const APPEARANCE_IMAGE_PREVIEW_SIZE: &str = "appearance preview size hover thumbnail pixels";
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

/// Decode a Slint key-event text into a readable key name. Slint encodes
/// special keys as control/private-use characters (Backspace → U+0008,
/// F5 → U+F708, …; see Slint's `key_codes` table), and these must be
/// translated before display or portal registration.
fn decode_slint_key(key: char) -> Option<String> {
    match key {
        '\u{0008}' => Some("Backspace".to_string()),
        '\u{0009}' => Some("Tab".to_string()),
        '\u{000a}' => Some("Return".to_string()),
        '\u{0010}' => Some("Shift".to_string()),
        '\u{0011}' => Some("Control".to_string()),
        '\u{0012}' => Some("Alt".to_string()),
        '\u{0013}' => Some("AltGr".to_string()),
        '\u{0014}' => Some("CapsLock".to_string()),
        '\u{0015}' => Some("ShiftR".to_string()),
        '\u{0016}' => Some("ControlR".to_string()),
        '\u{0017}' => Some("Meta".to_string()),
        '\u{0018}' => Some("MetaR".to_string()),
        '\u{0019}' => Some("Backtab".to_string()),
        '\u{0020}' => Some("Space".to_string()),
        '\u{007f}' => Some("Delete".to_string()),
        '\u{f700}' => Some("UpArrow".to_string()),
        '\u{f701}' => Some("DownArrow".to_string()),
        '\u{f702}' => Some("LeftArrow".to_string()),
        '\u{f703}' => Some("RightArrow".to_string()),
        '\u{f704}'..='\u{f71b}' => Some(format!("F{}", key as u32 - 0xf704 + 1)),
        '\u{f727}' => Some("Insert".to_string()),
        '\u{f729}' => Some("Home".to_string()),
        '\u{f72b}' => Some("End".to_string()),
        '\u{f72c}' => Some("PageUp".to_string()),
        '\u{f72d}' => Some("PageDown".to_string()),
        '\u{f72f}' => Some("ScrollLock".to_string()),
        '\u{f730}' => Some("Pause".to_string()),
        '\u{f731}' => Some("SysReq".to_string()),
        '\u{f734}' => Some("Stop".to_string()),
        '\u{f735}' => Some("Menu".to_string()),
        '\u{f748}' => Some("Back".to_string()),
        _ => None,
    }
}

/// Uppercase single-character keys (letters/digits) for display; named keys
/// like `Backspace` or `F5` keep their case.
fn display_key(key: &str) -> String {
    if key.chars().count() == 1 {
        key.to_uppercase()
    } else {
        key.to_string()
    }
}

/// Normalise a hotkey string captured from the settings UI. Slint's
/// special-key encodings are decoded to readable names (`\u{0008}` →
/// `Backspace`, `\u{F708}` → `F5`), and the key token is uppercased when it
/// is a single letter/digit so the assigned key displays uppercase (e.g.
/// `Ctrl+Alt+q` → `Ctrl+Alt+Q`).
///
/// A trailing `+` is the plus key itself (`Ctrl++` = Ctrl + the plus key)
/// when a `+` precedes it; a lone trailing `+` is a dangling separator from
/// capturing a modifier without a key and is dropped.
fn clean_hotkey_text(raw: &str) -> String {
    let cleaned: String = raw
        .chars()
        .map(|c| match decode_slint_key(c) {
            Some(name) => name,
            None => c.to_string(),
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
            Some((mods, key)) if !key.is_empty() => format!("{mods}+{}", display_key(key)),
            // Bare key: no `+` to split on.
            _ => display_key(&cleaned),
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

/// Delete cached hover-preview images so the next hover regenerates them at the
/// new `hover_image_preview_size`. The 36px list-cell thumbnails are left
/// alone. Best-effort and off the UI thread; a failure only leaves a stale
/// (possibly upscaled) preview in place until the next cache clear.
fn invalidate_image_previews(thumbnails_dir: std::path::PathBuf) {
    std::mem::drop(tokio::task::spawn_blocking(move || {
        let Ok(entries) = std::fs::read_dir(&thumbnails_dir) else {
            return;
        };
        for entry in entries.filter_map(|e| e.ok()) {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if !name.ends_with(PREVIEW_WEBP_SUFFIX) && !name.ends_with(PREVIEW_SVG_SUFFIX) {
                continue;
            }
            if let Err(e) = std::fs::remove_file(entry.path()) {
                tracing::warn!("invalidate_image_previews: {:?}: {e}", entry.path());
            }
        }
    }));
}

fn reapply_theme(
    main_ui: &slint::Weak<crate::AppWindow>,
    settings_win_ui: &slint::Weak<crate::SettingsWindow>,
    settings: &cliptoo_core::Settings,
    favicons_dir: std::path::PathBuf,
    fillers: crate::theme::ThemeFillers,
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
            // The tray and About window hold their own `Theme` globals; they
            // register here and are re-filled with the same resolved values.
            crate::theme::apply_theme_fillers(&fillers, &s_snap, is_dark, system_accent);
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
    fillers: crate::theme::ThemeFillers,
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
        let fillers = fillers.clone();
        tokio::spawn(async move {
            // Resolve first so the shared cache holds the freshly detected
            // OS accent; the settings swatch reads that cache. Without this,
            // a previously custom accent leaves the cache stale and "Clear"
            // shows the fallback color until a second click.
            let (is_dark, system_accent) = crate::theme::resolve_theme(&s_snap).await;
            let swatch = crate::theme::default_accent_color();
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
                // Re-fill the tray and About window globals too.
                crate::theme::apply_theme_fillers(&fillers, &s_settings, is_dark, system_accent);
                win.set_s_accent_color(swatch);
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

/// Show a toast on the settings window (its bottom overlay). Severity is one
/// of "info" | "warn" | "error".
fn show_settings_toast(win: &crate::SettingsWindow, message: &str, severity: &str) {
    win.set_toast_visible(false);
    win.set_toast_message(message.into());
    win.set_toast_severity(severity.into());
    win.set_toast_visible(true);
}

/// Font picker — native KDE font dialog via Qt (PyQt6).
///
/// The dialog runs in a child `python3` process; the blocking `.output()` call
/// runs on a background thread so the app stays responsive while the dialog is
/// open. `slint::spawn_local` polls the continuation on the UI thread, which
/// is what lets it capture the non-Send `Rc<RefCell<Settings>>`. A missing or
/// failing python3/PyQt6 is logged and surfaced via the settings toast instead
/// of silently doing nothing; cancelling the dialog is not an error.
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
        let sw = sw.clone();
        let settings_ui = settings_ui.clone();
        let s = s.clone();
        let p = p.clone();
        // The callback fires from the event loop, so spawning can only fail
        // if it is shutting down; ignore the JoinHandle (dropping it does not
        // cancel the future) but surface the rare spawn error.
        if let Err(e) = slint::spawn_local(async move {
            let result = tokio::task::spawn_blocking(move || {
                std::process::Command::new("python3")
                    .arg("-c")
                    .arg(script)
                    .output()
            })
            .await;

            // A successful run prints the family on stdout; anything else
            // (missing python3/PyQt6, a non-zero exit, a cancelled dialog)
            // yields "". The failure cases are distinguished from a plain
            // cancel so they can be surfaced.
            let (family, error): (String, Option<String>) = match result {
                Ok(Ok(output)) if output.status.success() => (
                    String::from_utf8_lossy(&output.stdout).trim().to_string(),
                    None,
                ),
                Ok(Ok(output)) => (
                    String::new(),
                    Some(format!(
                        "python3 exited {}: {}",
                        output.status,
                        String::from_utf8_lossy(&output.stderr).trim()
                    )),
                ),
                Ok(Err(e)) => (String::new(), Some(format!("failed to run python3: {e}"))),
                Err(e) => (
                    String::new(),
                    Some(format!("font picker task panicked: {e}")),
                ),
            };
            if let Some(err) = error.as_ref() {
                tracing::warn!("font-picker: {err}");
            }

            if family.is_empty() {
                if error.is_some()
                    && let Some(win) = sw.upgrade()
                {
                    show_settings_toast(
                        &win,
                        "Font picker failed (python3 + PyQt6 required)",
                        "error",
                    );
                }
                return;
            }

            {
                let mut settings = s.borrow_mut();
                settings.font_family.clone_from(&family);
                let _ = settings.save(&p);
            }
            apply_theme_to_windows(&settings_ui, &sw, |t| {
                t.set_font_family(family.as_str().into());
            });
            if let Some(win) = sw.upgrade() {
                win.set_s_font_family(family.as_str().into());
            }
        }) {
            tracing::warn!("font-picker: failed to schedule on the event loop: {e}");
        }
    });
}

/// Normalize a color-dialog result to an uppercase `#RRGGBB` string, returning
/// `None` when the output is not a 6-digit hex color (e.g. the user cancelled).
fn normalize_picker_hex(raw: &str) -> Option<String> {
    let hex = raw.trim().trim_start_matches('#');
    if hex.len() == 6 && hex.chars().all(|c| c.is_ascii_hexdigit()) {
        Some(format!("#{}", hex.to_ascii_uppercase()))
    } else {
        None
    }
}

/// Accent color picker — native KDE color dialog via `kdialog --getcolor`.
///
/// The dialog runs in a child `kdialog` process; the blocking `.output()` call
/// runs on a background thread so the app stays responsive while the dialog is
/// open. `slint::spawn_local` polls the continuation on the UI thread, which
/// is what lets it capture the non-Send `Rc<RefCell<Settings>>`. Cancelling the
/// dialog is not an error; a launch failure is logged and surfaced via the
/// settings toast instead of silently doing nothing.
fn setup_accent_picker(
    settings_win: &crate::SettingsWindow,
    main_ui: &crate::AppWindow,
    settings: &std::rc::Rc<std::cell::RefCell<cliptoo_core::Settings>>,
    settings_path: &std::path::Path,
    fillers: crate::theme::ThemeFillers,
) {
    let sw = settings_win.as_weak();
    let settings_ui = main_ui.as_weak();
    let s = settings.clone();
    let p = settings_path.to_path_buf();
    settings_win.on_accent_picker(move || {
        let sw = sw.clone();
        let settings_ui = settings_ui.clone();
        let s = s.clone();
        let p = p.clone();
        let fillers = fillers.clone();
        // Preselect the colour currently shown on the swatch. When "Clear" is
        // active this is the resolved OS/default accent, so the dialog still
        // opens on the colour the user actually sees.
        let current = sw.upgrade().map(|win| {
            let c = win.get_s_accent_color();
            format!("#{:02X}{:02X}{:02X}", c.red(), c.green(), c.blue())
        });
        if let Err(e) = slint::spawn_local(async move {
            let result = tokio::task::spawn_blocking(move || {
                let mut cmd = std::process::Command::new("kdialog");
                cmd.arg("--title").arg("Cliptoo accent color");
                if let Some(current) = current.as_deref() {
                    cmd.arg("--default").arg(current);
                }
                cmd.arg("--getcolor").output()
            })
            .await;

            // A successful run prints `#rrggbb`; a cancelled dialog exits
            // non-zero with nothing on stderr. A missing hex is treated as a
            // cancel; only genuine launch/output errors are surfaced.
            let (hex, error): (Option<String>, Option<String>) = match result {
                Ok(Ok(output)) if output.status.success() => (
                    normalize_picker_hex(&String::from_utf8_lossy(&output.stdout)),
                    None,
                ),
                Ok(Ok(output)) => {
                    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
                    (
                        None,
                        if stderr.is_empty() {
                            None
                        } else {
                            Some(format!("kdialog exited {}: {stderr}", output.status))
                        },
                    )
                }
                Ok(Err(e)) => (None, Some(format!("failed to run kdialog: {e}"))),
                Err(e) => (None, Some(format!("accent picker task panicked: {e}"))),
            };
            if let Some(err) = error.as_ref() {
                tracing::warn!("accent-picker: {err}");
            }

            let Some(hex) = hex else {
                if error.is_some()
                    && let Some(win) = sw.upgrade()
                {
                    show_settings_toast(&win, "Color picker failed (kdialog required)", "error");
                }
                return;
            };

            {
                let mut settings = s.borrow_mut();
                let (r, g, b) = crate::theme::parse_accent_hex(&hex);
                let (h, sat, val) = crate::theme::rgb_to_hsv(r, g, b);
                // The hex is authoritative; the HSV fields are kept in sync so
                // older settings files and any remaining readers stay valid.
                settings.accent_color.clone_from(&hex);
                settings.accent_hue = h;
                settings.accent_saturation = sat;
                settings.accent_value = val;
                let _ = settings.save(&p);
            }
            let (is_dark, _) = crate::theme::cached_resolved_theme();
            if let Some(ui) = settings_ui.upgrade() {
                crate::theme::fill_accent(&ui.global::<crate::Theme>(), &s.borrow(), is_dark, None);
            }
            if let Some(win) = sw.upgrade() {
                win.set_s_accent_color(crate::theme::accent_hex_to_color(&hex));
                let s = s.borrow();
                crate::theme::fill_accent(&win.global::<crate::Theme>(), &s, is_dark, None);
                crate::theme::apply_theme_fillers(&fillers, &s, is_dark, None);
            }
        }) {
            tracing::warn!("accent-picker: failed to schedule on the event loop: {e}");
        }
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
    thumbnails_dir: std::path::PathBuf,
    hotkey_tx: tokio::sync::watch::Sender<String>,
    retention_tx: tokio::sync::watch::Sender<cliptoo_core::maintenance::RetentionConfig>,
    blacklist_state: std::sync::Arc<std::sync::Mutex<Vec<String>>>,
    image_preview_size: std::sync::Arc<std::sync::atomic::AtomicU32>,
    fillers: crate::theme::ThemeFillers,
) {
    let s = settings.clone();
    let p = settings_path.to_path_buf();
    let settings_ui = main_ui.as_weak();
    let sw = settings_win.as_weak();
    let fillers = fillers.clone();
    let image_preview_size = image_preview_size.clone();
    let thumbnails_dir = thumbnails_dir.clone();
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
                    reapply_theme(&settings_ui, &sw, &s, favicons_dir.clone(), fillers.clone());
                }
                "font_family" => {
                    s.font_family = value.clone();
                    apply_theme_to_windows(&settings_ui, &sw, |t| {
                        t.set_font_family(value.as_str().into());
                    });
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
                        // The clipboard listener reads this shared value per
                        // thumbnail generation, and the preview popup reads the
                        // Theme token, so the change applies live.
                        image_preview_size.store(v, std::sync::atomic::Ordering::Relaxed);
                        apply_theme_to_windows(&settings_ui, &sw, |t| {
                            t.set_preview_size(v as f32);
                        });
                        // The preview window has its own per-window Theme global
                        // that apply_theme_to_windows does not reach.
                        crate::preview::set_preview_size(v as f32);
                        // Drop cached hover previews so the next hover rebuilds
                        // them at the new size instead of upscaling the old one.
                        invalidate_image_previews(thumbnails_dir.clone());
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

#[allow(clippy::too_many_arguments)]
pub fn setup_settings_window(
    ui: &crate::AppWindow,
    settings: &std::rc::Rc<std::cell::RefCell<cliptoo_core::Settings>>,
    dirs: &crate::app_dirs::AppDirs,
    hotkey_tx: tokio::sync::watch::Sender<String>,
    retention_tx: tokio::sync::watch::Sender<cliptoo_core::maintenance::RetentionConfig>,
    blacklist_state: std::sync::Arc<std::sync::Mutex<Vec<String>>>,
    image_preview_size: std::sync::Arc<std::sync::atomic::AtomicU32>,
    theme_fillers: crate::theme::ThemeFillers,
) -> crate::SettingsWindow {
    let settings_win = crate::SettingsWindow::new().expect("SettingsWindow creation");

    setup_close_persistence(&settings_win, ui, settings, &dirs.settings_path);
    init_settings_properties(&settings_win, settings);
    setup_maintenance_forwarding(&settings_win, ui);
    setup_settings_filter(&settings_win);
    setup_clear_accent(
        &settings_win,
        ui,
        settings,
        &dirs.settings_path,
        theme_fillers.clone(),
    );
    setup_open_log(&settings_win, &dirs.logs_dir);
    setup_settings_closing(&settings_win, ui, settings, &dirs.settings_path);
    setup_accent_picker(
        &settings_win,
        ui,
        settings,
        &dirs.settings_path,
        theme_fillers.clone(),
    );
    setup_font_picker(&settings_win, ui, settings, &dirs.settings_path);
    setup_setting_commit(
        &settings_win,
        ui,
        settings,
        &dirs.settings_path,
        dirs.favicons_dir.clone(),
        dirs.thumbnails_dir.clone(),
        hotkey_tx,
        retention_tx,
        blacklist_state,
        image_preview_size,
        theme_fillers,
    );
    setup_menu_open(&settings_win, ui);

    settings_win
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_picker_hex_accepts_hex_and_rejects_cancel() {
        assert_eq!(
            normalize_picker_hex("#7c6ee6\n").as_deref(),
            Some("#7C6EE6")
        );
        assert_eq!(normalize_picker_hex("7C6EE6").as_deref(), Some("#7C6EE6"));
        assert_eq!(normalize_picker_hex(""), None);
        assert_eq!(normalize_picker_hex("#12345"), None);
        assert_eq!(normalize_picker_hex("#gggggg"), None);
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
        // A bare path stays bare (name == path); a named entry keeps its name.
        assert_eq!(formatted, "code: /usr/bin/code, gedit");
        let round = parse_send_to_apps(&formatted);
        assert_eq!(round.len(), 2);
        assert_eq!(round[1].name, "gedit");
        assert_eq!(round[1].path, "gedit");
    }

    #[test]
    fn send_to_apps_keeps_name_for_directory_paths() {
        // A path with a directory derives its name from the stem, so it is
        // rendered back with the explicit `name: path` form.
        let apps = parse_send_to_apps("/opt/tools/meld");
        assert_eq!(format_send_to_apps(&apps), "meld: /opt/tools/meld");
    }

    #[test]
    fn parse_blacklist_trims_and_drops_empty() {
        assert_eq!(parse_blacklist(""), Vec::<String>::new());
        assert_eq!(
            parse_blacklist("  ,, org.kde.dolphin ,, kwrite "),
            vec!["org.kde.dolphin".to_string(), "kwrite".to_string()]
        );
    }

    #[test]
    fn clean_hotkey_text_uppercases_single_key() {
        assert_eq!(clean_hotkey_text("Ctrl+Alt+q"), "Ctrl+Alt+Q");
        assert_eq!(clean_hotkey_text("a"), "A");
        assert_eq!(clean_hotkey_text("F5"), "F5");
    }

    #[test]
    fn clean_hotkey_text_keeps_plus_key() {
        assert_eq!(clean_hotkey_text("Ctrl++"), "Ctrl++");
        assert_eq!(clean_hotkey_text("+"), "+");
    }

    #[test]
    fn clean_hotkey_text_drops_dangling_separator() {
        assert_eq!(clean_hotkey_text("Ctrl+"), "CTRL");
    }

    #[test]
    fn clean_hotkey_text_decodes_slint_special_keys() {
        assert_eq!(clean_hotkey_text("Ctrl+\u{0008}"), "Ctrl+Backspace");
        assert_eq!(clean_hotkey_text("Ctrl+\u{0009}"), "Ctrl+Tab");
        assert_eq!(clean_hotkey_text("\u{F708}"), "F5");
        assert_eq!(clean_hotkey_text("Ctrl+\u{F70f}"), "Ctrl+F12");
        assert_eq!(clean_hotkey_text("Ctrl+\u{F72c}"), "Ctrl+PageUp");
        assert_eq!(clean_hotkey_text("Ctrl+\u{0020}"), "Ctrl+Space");
    }

    #[test]
    fn decode_slint_key_maps_f_keys_and_pass_through() {
        assert_eq!(decode_slint_key('\u{f704}'), Some("F1".to_string()));
        assert_eq!(decode_slint_key('\u{f71b}'), Some("F24".to_string()));
        assert_eq!(decode_slint_key('a'), None);
        assert_eq!(decode_slint_key('\u{0008}'), Some("Backspace".to_string()));
    }

    #[test]
    fn display_key_uppercases_only_single_chars() {
        assert_eq!(display_key("q"), "Q");
        assert_eq!(display_key("F5"), "F5");
        assert_eq!(display_key("PageUp"), "PageUp");
    }

    #[test]
    fn idx_of_matches_case_insensitively() {
        let options = ["Right Alt", "Left Alt", "Control"];
        assert_eq!(idx_of("Left Alt", &options), 1);
        assert_eq!(idx_of("left alt", &options), 1);
        assert_eq!(idx_of("CONTROL", &options), 2);
    }

    #[test]
    fn idx_of_falls_back_to_default_for_unknown_value() {
        assert_eq!(
            idx_of("Super Key", &["Right Alt", "Left Alt", "Control"]),
            0
        );
        assert_eq!(idx_of("", &["Debug", "Info", "Warn", "Error"]), 0);
    }
}
