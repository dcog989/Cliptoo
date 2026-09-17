//! Instant-filter matching for the settings search box. Slint has no substring
//! matching, so each option row carries a keyword list that is matched here.

use slint::ComponentHandle;

/// Instant-filter keyword lists, one per setting row, grouped by section.
/// The header search box is matched against these (case-insensitive) by
/// `apply_settings_filter`; a row shows when the query matches any keyword.
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
pub(super) fn apply_settings_filter(win: &crate::SettingsWindow, query: &str) {
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

/// Instant filter search: the header query is matched against each option
/// row's keywords and drives the row visibility. Runs on the UI thread (the
/// callback fires from Slint's edited handler), so setters are safe here.
pub(super) fn setup_settings_filter(settings_win: &crate::SettingsWindow) {
    let sw = settings_win.as_weak();
    settings_win.on_filter_changed(move |query: slint::SharedString| {
        if let Some(win) = sw.upgrade() {
            apply_settings_filter(&win, &query);
        }
    });
}
