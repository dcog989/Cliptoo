//! Theme resolution and token filling for every Slint window.
//!
//! System detection lives in [`detect`]; the WCAG/OKLCH accent math lives in
//! [`contrast`]. This module owns the resolved-theme cache, the `Theme`-global
//! fill functions, and the filler registry for windows other than the main and
//! settings ones.

use crate::Theme;
use cliptoo_core::Settings;
use cliptoo_core::color::srgb_bytes_to_oklch;
use slint::{Color, ComponentHandle, SharedString};
use std::sync::Mutex;

use self::contrast::{
    accent_sibling, bg_primary, contrast_safe_accent, relative_luminance, select_accent_fg,
};

mod contrast;
mod detect;

pub use detect::{detect_system_accent, detect_system_dark};

/// Fallback accent RGB used when a stored hex is malformed. Empty strings are
/// valid — they mean "use the OS accent", which is only known at runtime.
const DEFAULT_ACCENT: (u8, u8, u8) = (0x7C, 0x6E, 0xE6);

/// Alpha channel (0–255) for the drop-shadow color shared by menus. Matches
/// the `#00000030` default in Theme.slint so shadows stay translucent instead
/// of rendering as opaque black halos.
const SHADOW_ALPHA: u8 = 0x30;

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

    // Surface the accent is applied on; the accent must keep a minimum
    // contrast against it in both theme modes.
    let (bg_r, bg_g, bg_b) = bg_primary(is_dark);
    let bg_lum = relative_luminance(bg_r, bg_g, bg_b);

    // Base accent: the system accent in "System" theme mode, otherwise the
    // user-picked color.
    let (accent_rgb, (accent_l, accent_c, accent_h)) = if let Some((sr, sg, sb)) = system_accent {
        tracing::debug!("detected system accent: #{sr:02X}{sg:02X}{sb:02X}");
        ((sr, sg, sb), srgb_bytes_to_oklch(sr, sg, sb))
    } else {
        let (r, g, b) = parse_accent_hex(&settings.accent_color);
        ((r, g, b), srgb_bytes_to_oklch(r, g, b))
    };

    // Buttons, pills and borders use a lightness-adjusted accent that keeps
    // contrast with the surface; hovered rows keep the exact accent so the
    // user sees the color they actually picked.
    let (accent_is_dark, (accent_r, accent_g, accent_b)) =
        contrast_safe_accent(accent_rgb, accent_l, accent_c, accent_h, bg_lum);
    let accent = Color::from_rgb_u8(accent_r, accent_g, accent_b);
    let (raw_r, raw_g, raw_b) = accent_rgb;
    let raw_accent = Color::from_rgb_u8(raw_r, raw_g, raw_b);
    let raw_is_dark = select_accent_fg(relative_luminance(raw_r, raw_g, raw_b));
    // Content sitting on an accent background flips to this for contrast.
    t.set_accent_fg(if accent_is_dark {
        Color::from_rgb_u8(0xFF, 0xFF, 0xFF)
    } else {
        Color::from_rgb_u8(0x00, 0x00, 0x00)
    });
    // Hovered rows use the exact (unadjusted) accent as their background, so
    // their foreground is chosen for contrast against that accent instead.
    t.set_row_hover_fg(if raw_is_dark {
        Color::from_rgb_u8(0xFF, 0xFF, 0xFF)
    } else {
        Color::from_rgb_u8(0x00, 0x00, 0x00)
    });
    t.set_accent_primary(accent);
    t.set_border_accent(accent_sibling(accent_h, accent_c, border_accent_l, 0.40));
    t.set_bg_row_hover(raw_accent);
}

/// Apply pre-resolved theme tokens to any `Theme` global handle.
/// Used by the window, the tray, and any other component with `Theme`.
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
        t.set_bg_row_selected(Color::from_rgb_u8(0x28, 0x28, 0x28));
        t.set_bg_input(Color::from_rgb_u8(0x24, 0x24, 0x24));
        t.set_fg_primary(Color::from_rgb_u8(0xE4, 0xE4, 0xE4));
        t.set_fg_secondary(Color::from_rgb_u8(0xA1, 0xA1, 0xA1));
        t.set_fg_clip_list(Color::from_rgb_u8(0xB4, 0xB4, 0xB4));
        t.set_fg_muted(Color::from_rgb_u8(0x63, 0x63, 0x63));
        t.set_fg_danger(Color::from_rgb_u8(0xE7, 0x4C, 0x3C));
        t.set_fg_success(Color::from_rgb_u8(0x2E, 0xCC, 0x71));
        t.set_fg_warning(Color::from_rgb_u8(0xF3, 0x9C, 0x12));
        t.set_fg_bookmarked(Color::from_rgb_u8(0xE5, 0xB5, 0x67));
        t.set_border_subtle(Color::from_rgb_u8(0x44, 0x44, 0x44));
        t.set_shadow(Color::from_argb_u8(SHADOW_ALPHA, 0x00, 0x00, 0x00));
    } else {
        t.set_bg_primary(Color::from_rgb_u8(bg_r, bg_g, bg_b));
        t.set_bg_header(Color::from_rgb_u8(0xE8, 0xE8, 0xE8));
        t.set_bg_row_alt(Color::from_rgb_u8(0xEE, 0xEE, 0xEE));
        t.set_bg_row_selected(Color::from_rgb_u8(0xDD, 0xDD, 0xDD));
        t.set_bg_input(Color::from_rgb_u8(0xFF, 0xFF, 0xFF));
        t.set_fg_primary(Color::from_rgb_u8(0x1C, 0x1C, 0x1C));
        t.set_fg_secondary(Color::from_rgb_u8(0x55, 0x55, 0x55));
        t.set_fg_clip_list(Color::from_rgb_u8(0x55, 0x55, 0x55));
        t.set_fg_muted(Color::from_rgb_u8(0x99, 0x99, 0x99));
        t.set_fg_danger(Color::from_rgb_u8(0xC0, 0x39, 0x2B));
        t.set_fg_success(Color::from_rgb_u8(0x27, 0xAE, 0x60));
        t.set_fg_warning(Color::from_rgb_u8(0xE6, 0x7E, 0x22));
        t.set_fg_bookmarked(Color::from_rgb_u8(0xB8, 0x86, 0x0B));
        t.set_border_subtle(Color::from_rgb_u8(0xD0, 0xD0, 0xD0));
        t.set_shadow(Color::from_argb_u8(SHADOW_ALPHA, 0x00, 0x00, 0x00));
    }

    t.set_font_family(SharedString::from(&*settings.font_family));
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

fn cache_resolved_theme(is_dark: bool, system_accent: Option<(u8, u8, u8)>) {
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

/// Convert sRGB (0–255) to HSV (hue 0–360, saturation 0–1, value 0–1).
/// Used by the settings accent picker to keep the persisted HSV fields in
/// sync with the picked hex.
pub(crate) fn rgb_to_hsv(r: u8, g: u8, b: u8) -> (f64, f64, f64) {
    let r = r as f64 / 255.0;
    let g = g as f64 / 255.0;
    let b = b as f64 / 255.0;
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let d = max - min;
    let h = if d == 0.0 {
        0.0
    } else if max == r {
        60.0 * ((g - b) / d % 6.0)
    } else if max == g {
        60.0 * ((b - r) / d + 2.0)
    } else {
        60.0 * ((r - g) / d + 4.0)
    };
    let h = if h < 0.0 { h + 360.0 } else { h };
    let s = if max == 0.0 { 0.0 } else { d / max };
    (h, s, max)
}

/// Resolve the dark-mode and system-accent context for the given settings.
/// Side-effect: caches the result for `cached_resolved_theme`.
pub async fn resolve_theme(settings: &Settings) -> ResolvedTheme {
    let is_dark = match settings.theme.as_str() {
        "Light" => false,
        "Dark" => true,
        _ => detect_system_dark().await.unwrap_or(true),
    };
    // Follow the OS accent only when no custom accent has been chosen
    // (empty accent_color = "use OS default"). A user-picked accent always
    // wins, regardless of theme mode; theme mode only controls dark vs light.
    let system_accent = if settings.accent_color.trim().is_empty() {
        detect_system_accent().await
    } else {
        None
    };
    cache_resolved_theme(is_dark, system_accent);
    (is_dark, system_accent)
}

/// Apply theme tokens to the Slint UI global `Theme` of the main window.
/// Returns the resolved (is_dark, system_accent) pair so callers can fill the
/// other windows' `Theme` globals with the same values.
pub async fn apply_theme(ui: &crate::AppWindow, settings: &Settings) -> ResolvedTheme {
    let (is_dark, system_accent) = resolve_theme(settings).await;
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
register_theme_filler!(register_tray_theme_filler, crate::CliptooTray);
