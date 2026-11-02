//! Theme resolution and token filling for every Slint window.
//!
//! System detection lives in [`detect`]; the APCA/OKLCH accent math lives in
//! [`contrast`]. This module owns the resolved-theme cache, the `Theme`-global
//! fill functions, and the filler registry for windows other than the main and
//! settings ones.

use crate::Theme;
use cliptoo_core::Settings;
use cliptoo_core::color::srgb_bytes_to_oklch;
use slint::{Color, ComponentHandle, SharedString};
use std::sync::Mutex;

use self::contrast::{
    FILL_MIN_LC, TEXT_MIN_LC, accent_sibling, bg_primary, contrast_safe_accent, match_highlight,
    text_surface,
};

mod contrast;
mod detect;
mod palette;

pub use detect::{detect_system_accent, detect_system_dark};

/// Fallback accent RGB used when a stored hex is malformed. Empty strings are
/// valid — they mean "use the OS accent", which is only known at runtime.
const DEFAULT_ACCENT: (u8, u8, u8) = (0x7C, 0x6E, 0xE6);

/// Alpha channel (0–255) for the drop-shadow color shared by menus. Matches
/// the `#00000030` default in Theme.slint so shadows stay translucent instead
/// of rendering as opaque black halos.
const SHADOW_ALPHA: u8 = 0x30;

/// OKLCH chroma fraction of the accent kept by the accent border. Low so the
/// border reads as a tint rather than a second block of the accent.
const BORDER_CHROMA_SCALE: f64 = 0.40;

/// Page text colours that are not accent-derived. They are mode-specific so
/// they can clear the same APCA text floor as `accent-primary`.
const DANGER_FG_DARK: (u8, u8, u8) = (0xFF, 0x9A, 0x8A);
const DANGER_FG_LIGHT: (u8, u8, u8) = (0xC9, 0x38, 0x26);
const LINK_FG_DARK: (u8, u8, u8) = (0x8F, 0xBE, 0xFF);
const LINK_FG_LIGHT: (u8, u8, u8) = (0x2F, 0x6F, 0xC9);

/// Convert 0–255 channels to a Slint colour.
fn rgb((r, g, b): (u8, u8, u8)) -> Color {
    Color::from_rgb_u8(r, g, b)
}

/// Parse a `#RRGGBB` hex string (leading `#` optional) into `(r, g, b)`.
/// Returns `DEFAULT_ACCENT` for malformed input.
pub(crate) fn parse_accent_hex(hex: &str) -> (u8, u8, u8) {
    let hex = hex.trim().trim_start_matches('#');
    if hex.len() == 6
        && let Ok(v) = u32::from_str_radix(hex, 16)
    {
        (
            ((v >> 16) & 0xFF) as u8,
            ((v >> 8) & 0xFF) as u8,
            (v & 0xFF) as u8,
        )
    } else {
        DEFAULT_ACCENT
    }
}

/// Convert a stored accent hex string to a `slint::Color`.
pub(crate) fn accent_hex_to_color(hex: &str) -> Color {
    let (r, g, b) = parse_accent_hex(hex);
    Color::from_rgb_u8(r, g, b)
}

/// The fallback/default accent color: the detected OS/system accent when
/// available, otherwise the core settings default (`#7C6EE6`). This is what
/// "Clear" shows in the settings, since clearing means "use the OS default".
pub(crate) fn default_accent_color() -> Color {
    let (r, g, b) = cached_resolved_theme().1.unwrap_or(DEFAULT_ACCENT);
    Color::from_rgb_u8(r, g, b)
}

/// Apply only the accent-derived tokens (accent, its foregrounds, and the
/// hover background) to a `Theme` global, leaving the rest of the palette
/// untouched. Used by the settings accent picker so a live preview updates
/// just the accent instead of recomputing the whole theme.
pub fn fill_accent(
    t: &Theme,
    settings: &Settings,
    is_dark: bool,
    system_accent: Option<(u8, u8, u8)>,
) {
    let border_accent_l = if is_dark { 0.54 } else { 0.40 };
    // Hovered-row surface. The accent's hue at a fixed OKLCH lightness well
    // above the other row states (page 0.21 / alt 0.25 / selected 0.32), at a
    // step (≈2:1) well under the 4:1 block the solid accent gave. Muted
    // content lifts to `fg-clip-list` on hover; `fg-muted` only reaches 1.7:1
    // here.
    let row_hover_l = if is_dark { 0.42 } else { 0.84 };
    // Selected-row surface: the same accent hue, one step darker than the hover
    // tint so at-rest muted icons/text stay legible on it.
    let row_selected_l = if is_dark { 0.32 } else { 0.88 };

    // Worst surface the accent text is drawn on: the lightest in dark mode,
    // the darkest in light mode. Adjusting against it keeps the text grade
    // above the floor on the page, the header, the alternate row and inputs.
    let bg = text_surface(is_dark);

    // Base accent: the system accent in "System" theme mode, otherwise the
    // user-picked color.
    let (accent_rgb, (accent_l, accent_c, accent_h)) = if let Some((sr, sg, sb)) = system_accent {
        tracing::debug!("detected system accent: #{sr:02X}{sg:02X}{sb:02X}");
        ((sr, sg, sb), srgb_bytes_to_oklch(sr, sg, sb))
    } else {
        let (r, g, b) = parse_accent_hex(&settings.accent_color);
        ((r, g, b), srgb_bytes_to_oklch(r, g, b))
    };

    // Text grade for headings/links/focus rings, shape grade for fills.
    let (_, (accent_r, accent_g, accent_b)) =
        contrast_safe_accent(accent_rgb, accent_l, accent_c, accent_h, bg, TEXT_MIN_LC);
    let (fill_is_dark, (fill_r, fill_g, fill_b)) =
        contrast_safe_accent(accent_rgb, accent_l, accent_c, accent_h, bg, FILL_MIN_LC);
    let fill = Color::from_rgb_u8(fill_r, fill_g, fill_b);
    // Content sitting on a filled accent shape flips to this for contrast, so
    // it is chosen against the exact color that shape is drawn with.
    t.set_accent_fg(if fill_is_dark {
        Color::from_rgb_u8(0xFF, 0xFF, 0xFF)
    } else {
        Color::from_rgb_u8(0x00, 0x00, 0x00)
    });
    t.set_accent_primary(Color::from_rgb_u8(accent_r, accent_g, accent_b));
    t.set_accent_fill(fill);
    t.set_border_accent(accent_sibling(
        accent_h,
        accent_c * BORDER_CHROMA_SCALE,
        border_accent_l,
    ));
    // Row tints keep the accent's C/L ratio: OKLCH chroma is absolute, so the
    // picked chroma at a lower lightness would read as more colorful.
    let tint_chroma = |l: f64| {
        if accent_l > 0.0 {
            accent_c * l / accent_l
        } else {
            accent_c
        }
    };
    t.set_bg_row_hover(accent_sibling(
        accent_h,
        tint_chroma(row_hover_l),
        row_hover_l,
    ));
    t.set_bg_row_selected(accent_sibling(
        accent_h,
        tint_chroma(row_selected_l),
        row_selected_l,
    ));
    // FTS match spans: the accent's hue at a fixed strong chroma and a high
    // contrast against the surface, so a match never reads as body text.
    t.set_match_highlight(match_highlight(accent_h, bg));
}

/// Apply pre-resolved theme tokens to any `Theme` global handle.
/// Used by every window and any other component with `Theme`.
pub fn fill_theme(
    t: &Theme,
    settings: &Settings,
    is_dark: bool,
    system_accent: Option<(u8, u8, u8)>,
) {
    fill_accent(t, settings, is_dark, system_accent);

    let (bg_r, bg_g, bg_b) = bg_primary(is_dark);

    if is_dark {
        t.set_bg_primary(Color::from_rgb_u8(bg_r, bg_g, bg_b));
        t.set_bg_header(Color::from_rgb_u8(0x12, 0x12, 0x12));
        t.set_bg_row_alt(Color::from_rgb_u8(0x1C, 0x1C, 0x1C));
        t.set_bg_input(Color::from_rgb_u8(0x24, 0x24, 0x24));
        t.set_bg_track(Color::from_rgb_u8(0x55, 0x55, 0x55));
        t.set_fg_primary(Color::from_rgb_u8(0xE4, 0xE4, 0xE4));
        t.set_fg_secondary(Color::from_rgb_u8(0xA1, 0xA1, 0xA1));
        t.set_fg_clip_list(Color::from_rgb_u8(0xB4, 0xB4, 0xB4));
        t.set_fg_muted(Color::from_rgb_u8(0x63, 0x63, 0x63));
        t.set_fg_bookmarked(Color::from_rgb_u8(0xE5, 0xB5, 0x67));
        t.set_danger_fg(rgb(DANGER_FG_DARK));
        t.set_link_fg(rgb(LINK_FG_DARK));
        t.set_border_subtle(Color::from_rgb_u8(0x44, 0x44, 0x44));
        t.set_shadow(Color::from_argb_u8(SHADOW_ALPHA, 0x00, 0x00, 0x00));
    } else {
        t.set_bg_primary(Color::from_rgb_u8(bg_r, bg_g, bg_b));
        t.set_bg_header(Color::from_rgb_u8(0xE8, 0xE8, 0xE8));
        t.set_bg_row_alt(Color::from_rgb_u8(0xEE, 0xEE, 0xEE));
        t.set_bg_input(Color::from_rgb_u8(0xFF, 0xFF, 0xFF));
        // Slider / toggle-off track. Must be set per mode (the `Theme.slint`
        // default is the dark value). #C4C4C4 only reached Lc 26 against the
        // page; this holds Lc 54 and keeps the white knob at Lc 66.
        t.set_bg_track(Color::from_rgb_u8(0x8E, 0x8E, 0x8E));
        t.set_fg_primary(Color::from_rgb_u8(0x1C, 0x1C, 0x1C));
        t.set_fg_secondary(Color::from_rgb_u8(0x55, 0x55, 0x55));
        t.set_fg_clip_list(Color::from_rgb_u8(0x55, 0x55, 0x55));
        t.set_fg_muted(Color::from_rgb_u8(0x99, 0x99, 0x99));
        t.set_fg_bookmarked(Color::from_rgb_u8(0xB8, 0x86, 0x0B));
        t.set_danger_fg(rgb(DANGER_FG_LIGHT));
        t.set_link_fg(rgb(LINK_FG_LIGHT));
        t.set_border_subtle(Color::from_rgb_u8(0xD0, 0xD0, 0xD0));
        t.set_shadow(Color::from_argb_u8(SHADOW_ALPHA, 0x00, 0x00, 0x00));
    }

    // Clip-type palette (`palette`): one hue/level table rendered per mode, so
    // the dark and light hexes can never drift apart.
    let type_color = |role: palette::TypeColor| {
        let [r, g, b] = role.rgb(is_dark);
        Color::from_rgb_u8(r, g, b)
    };
    t.set_fg_type_danger(type_color(palette::DANGER));
    t.set_fg_type_video(type_color(palette::VIDEO));
    t.set_fg_type_audio(type_color(palette::AUDIO));
    t.set_fg_type_image(type_color(palette::IMAGE));
    t.set_fg_type_archive(type_color(palette::ARCHIVE));
    t.set_fg_type_path(type_color(palette::PATH));
    t.set_fg_type_folder(type_color(palette::FOLDER));
    t.set_fg_type_text_file(type_color(palette::TEXT_FILE));
    t.set_fg_type_rich(type_color(palette::RICH));
    t.set_fg_type_document(type_color(palette::DOCUMENT));
    t.set_fg_type_link(type_color(palette::LINK));
    t.set_fg_type_code(type_color(palette::CODE));
    t.set_fg_type_dev(type_color(palette::DEV));
    t.set_fg_type_color(type_color(palette::COLOR));
    t.set_fg_type_data(type_color(palette::DATA));
    t.set_fg_type_generic(type_color(palette::GENERIC));

    t.set_font_family(SharedString::from(&*settings.font_family));
    t.set_color_code_clip_types(settings.color_code_clip_types);
    t.set_clip_list_font_size(settings.font_size as f32);
    t.set_preview_font_size(settings.preview_font_size as f32);
    t.set_preview_size(settings.hover_image_preview_size as f32);

    t.set_hover_preview_delay(settings.hover_preview_delay as i64);

    t.set_row_height(crate::positioning::row_height(&settings.clip_item_padding) as f32);
}

/// A resolved theme context: whether the UI should be dark and, if in
/// "System" theme mode, the detected system accent color in 0–255.
type ResolvedTheme = (bool, Option<(u8, u8, u8)>);

/// The most recently resolved (is_dark, system_accent) pair, shared so any
/// window with a `Theme` global can be (re-)filled without re-querying the
/// portal. Slint globals are per-window-instance, so every window's `Theme`
/// global must be filled individually with the same values.
static RESOLVED_THEME: Mutex<Option<ResolvedTheme>> = Mutex::new(None);

pub(crate) fn cache_resolved_theme(is_dark: bool, system_accent: Option<(u8, u8, u8)>) {
    // A poisoned lock still holds a valid value; recover it so a panic while
    // holding the guard never drops the freshly resolved theme.
    let mut cached = RESOLVED_THEME
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    *cached = Some((is_dark, system_accent));
}

/// The last resolved theme context, or `(dark, no accent)` if never resolved.
pub fn cached_resolved_theme() -> ResolvedTheme {
    RESOLVED_THEME
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .unwrap_or((true, None))
}

/// Resolve the dark-mode and system-accent context for the given settings.
/// The caller writes [`cache_resolved_theme`] after deciding the result is the
/// newest one, so a slow older resolve cannot replace a newer cached value.
pub async fn resolve_theme(settings: &Settings) -> ResolvedTheme {
    // The two portal reads are independent; run them together so a portal that
    // stops replying costs one 2-second timeout, not two.
    let dark = async {
        match settings.theme.as_str() {
            "Light" => false,
            "Dark" => true,
            _ => detect_system_dark().await.unwrap_or(true),
        }
    };
    let accent = async {
        // Follow the OS accent only when no custom accent has been chosen
        // (empty accent_color = "use OS default"). A user-picked accent always
        // wins, regardless of theme mode; theme mode only controls dark vs light.
        if settings.accent_color.trim().is_empty() {
            detect_system_accent().await
        } else {
            None
        }
    };
    tokio::join!(dark, accent)
}

/// Apply theme tokens to the Slint UI global `Theme` of the main window.
/// Returns the resolved (is_dark, system_accent) pair so callers can fill the
/// other windows' `Theme` globals with the same values.
pub async fn apply_theme(ui: &crate::AppWindow, settings: &Settings) -> ResolvedTheme {
    let (is_dark, system_accent) = resolve_theme(settings).await;
    cache_resolved_theme(is_dark, system_accent);
    fill_theme(&ui.global::<Theme>(), settings, is_dark, system_accent);
    (is_dark, system_accent)
}

/// A Theme re-fill closure registered for a window that holds its own `Theme`
/// global. `reapply_theme`/`setup_clear_accent` invoke these on the UI thread
/// so a live theme/accent change reaches every such window — not just the main
/// and settings windows. Slint globals are per-window-instance, and
/// `global::<T>()` is an inherent method on each generated component (not a
/// trait method), so each window type gets its own registrar below.
type ThemeFillFn = dyn Fn(&Settings, bool, Option<(u8, u8, u8)>) + Send;

pub type ThemeFillers = std::sync::Arc<std::sync::Mutex<Vec<Box<ThemeFillFn>>>>;

/// New, empty theme-filler registry. Windows register themselves after their
/// own `Theme` global is first filled; a theme change then re-fills them.
pub fn new_theme_fillers() -> ThemeFillers {
    std::sync::Arc::new(std::sync::Mutex::new(Vec::new()))
}

/// Re-fill the `Theme` global of every registered window. Must run on the UI
/// thread: the registrars upgrade weak window handles.
pub fn apply_theme_fillers(
    fillers: &ThemeFillers,
    settings: &Settings,
    is_dark: bool,
    system_accent: Option<(u8, u8, u8)>,
) {
    for fill in fillers
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .iter()
    {
        fill(settings, is_dark, system_accent);
    }
}

macro_rules! register_theme_filler {
    ($name:ident, $ty:ty) => {
        pub fn $name(fillers: &ThemeFillers, window: &slint::Weak<$ty>) {
            let window = window.clone();
            fillers
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .push(Box::new(move |s, is_dark, accent| {
                    if let Some(win) = window.upgrade() {
                        fill_theme(&win.global::<crate::Theme>(), s, is_dark, accent);
                    }
                }));
        }
    };
}

register_theme_filler!(register_about_theme_filler, crate::AboutWindow);
register_theme_filler!(register_preview_theme_filler, crate::PreviewWindow);
register_theme_filler!(register_edit_theme_filler, crate::EditWindow);

#[cfg(test)]
mod tests {
    use super::contrast::{
        BG_HEADER_DARK, BG_HEADER_LIGHT, TEXT_MIN_LC, apca_contrast, bg_primary,
    };
    use super::{DANGER_FG_DARK, DANGER_FG_LIGHT, LINK_FG_DARK, LINK_FG_LIGHT};

    #[test]
    fn page_status_colours_clear_the_text_floor() {
        for (fg, bg) in [
            (DANGER_FG_DARK, bg_primary(true)),
            (DANGER_FG_DARK, BG_HEADER_DARK),
            (LINK_FG_DARK, bg_primary(true)),
            (LINK_FG_DARK, BG_HEADER_DARK),
            (DANGER_FG_LIGHT, bg_primary(false)),
            (DANGER_FG_LIGHT, BG_HEADER_LIGHT),
            (LINK_FG_LIGHT, bg_primary(false)),
            (LINK_FG_LIGHT, BG_HEADER_LIGHT),
        ] {
            let lc = apca_contrast(fg, bg).abs();
            assert!(lc >= TEXT_MIN_LC - 0.1, "{fg:?} on {bg:?}: Lc {lc}");
        }
    }
}
