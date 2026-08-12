use crate::Theme;
use cliptoo_core::Settings;
use cliptoo_core::color::{oklch_to_srgb_bytes, srgb_bytes_to_oklch};
use slint::{Color, ComponentHandle, SharedString};
use std::sync::Mutex;

/// Detect the system color-scheme preference via xdg-desktop-portal.
/// Returns `true` for dark, `false` for light, `None` if undetectable.
/// Portal's `Read` returns `(v)` where `v` is the value; zbus unwraps
/// the variant transparently, so we deserialize as `(u32,)`.
pub async fn detect_system_dark() -> Option<bool> {
    let conn = crate::dbus::session().await.ok()?;
    let msg = conn
        .call_method(
            Some("org.freedesktop.portal.Desktop"),
            "/org/freedesktop/portal/desktop",
            Some("org.freedesktop.portal.Settings"),
            "Read",
            &("org.freedesktop.appearance", "color-scheme"),
        )
        .await
        .ok()?;

    let (val,): (u32,) = msg.body().deserialize().ok()?;
    match val {
        1 => Some(true),
        2 => Some(false),
        _ => None,
    }
}

/// Try to read the KDE Plasma 6 accent color from `~/.config/kdeglobals`.
/// Plasma 6 writes the user's chosen accent color as `AccentColor=r,g,b,a`
/// under `[General]` in this file.
fn read_kdeglobals_accent() -> Option<(u8, u8, u8)> {
    let path = dirs::home_dir()?.join(".config").join("kdeglobals");

    let text = std::fs::read_to_string(path).ok()?;
    let mut in_general = false;
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed == "[General]" {
            in_general = true;
            continue;
        }
        if trimmed.starts_with('[') {
            in_general = false;
            continue;
        }
        if in_general && trimmed.starts_with("AccentColor=") {
            let parts: Vec<&str> = trimmed[12..].split(',').collect();
            if parts.len() >= 3 {
                let r = parts[0].trim().parse::<u8>().ok()?;
                let g = parts[1].trim().parse::<u8>().ok()?;
                let b = parts[2].trim().parse::<u8>().ok()?;
                return Some((r, g, b));
            }
        }
    }
    None
}

/// Detect the system accent color, trying xdg-desktop-portal first,
/// then falling back to `~/.config/kdeglobals` on KDE Plasma 6.
/// Returns `(r, g, b)` in 0–255, or `None` if undetectable.
pub async fn detect_system_accent() -> Option<(u8, u8, u8)> {
    // Try the portal (org.freedesktop.appearance.accent-color)
    if let Ok(conn) = crate::dbus::session().await
        && let Ok(msg) = conn
            .call_method(
                Some("org.freedesktop.portal.Desktop"),
                "/org/freedesktop/portal/desktop",
                Some("org.freedesktop.portal.Settings"),
                "Read",
                &("org.freedesktop.appearance", "accent-color"),
            )
            .await
        && let Ok((r, g, b)) = msg.body().deserialize::<(f64, f64, f64)>()
    {
        return Some((
            (r.clamp(0.0, 1.0) * 255.0).round() as u8,
            (g.clamp(0.0, 1.0) * 255.0).round() as u8,
            (b.clamp(0.0, 1.0) * 255.0).round() as u8,
        ));
    }
    // Fallback: read kdeglobals directly
    read_kdeglobals_accent()
}

/// Fallback accent RGB used when a stored hex is malformed. Empty strings are
/// valid — they mean "use the OS accent", which is only known at runtime.
const DEFAULT_ACCENT: (u8, u8, u8) = (0x7C, 0x6E, 0xE6);

/// Primary surface the accent is applied on (background). The accent must
/// keep a minimum contrast against this in both theme modes.
const BG_PRIMARY_DARK: (u8, u8, u8) = (0x18, 0x18, 0x18);
const BG_PRIMARY_LIGHT: (u8, u8, u8) = (0xF5, 0xF5, 0xF5);

/// Alpha channel (0–255) for the drop-shadow color shared by menus. Matches
/// the `#00000030` default in Theme.slint so shadows stay translucent instead
/// of rendering as opaque black halos.
const SHADOW_ALPHA: u8 = 0x30;

/// Minimum WCAG contrast ratio (AA, normal text) required between the accent
/// and the surface it is applied to.
const MIN_CONTRAST_RATIO: f64 = 4.5;

/// Binary-search iterations for the OKLCH-lightness contrast adjustment.
/// Precision is bounded by the lightness range (0–1) divided by 2^iters.
const CONTRAST_SEARCH_ITERS: u32 = 32;

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

/// Derive an accent-tinted color at a fixed OKLCH lightness `l`, scaling the
/// base accent's chroma by `chroma_scale`. Used for the subtle accent border,
/// so a picked Neon stays vibrant and a picked Muted stays restrained.
fn accent_sibling(hue: f64, base_chroma: f64, l: f64, chroma_scale: f64) -> Color {
    let [r, g, b] = oklch_to_srgb_bytes(l, base_chroma * chroma_scale, hue);
    Color::from_rgb_u8(r, g, b)
}

/// WCAG 2.x relative luminance (0–1) of an sRGB color.
fn relative_luminance(r: u8, g: u8, b: u8) -> f64 {
    fn channel(c: u8) -> f64 {
        let c = c as f64 / 255.0;
        if c <= 0.040_45 {
            c / 12.92
        } else {
            ((c + 0.055) / 1.055).powf(2.4)
        }
    }
    0.2126 * channel(r) + 0.7152 * channel(g) + 0.0722 * channel(b)
}

/// WCAG contrast ratio between two relative luminances (order-independent).
fn contrast_ratio(a: f64, b: f64) -> f64 {
    let (hi, lo) = if a >= b { (a, b) } else { (b, a) };
    (hi + 0.05) / (lo + 0.05)
}

/// Relative luminance of the accent at OKLCH lightness `l`, holding the
/// accent's chroma and hue fixed.
fn accent_luminance(l: f64, c: f64, h: f64) -> f64 {
    let [r, g, b] = oklch_to_srgb_bytes(l, c, h);
    relative_luminance(r, g, b)
}

/// Adjust the accent's OKLCH lightness (hue and chroma held fixed) so it keeps
/// `MIN_CONTRAST_RATIO` against the surface it is applied to. The accent is
/// pushed away from the surface's luminance — lightened on dark surfaces,
/// darkened on light ones — so its hue and saturation survive.
///
/// Returns `(use_white_foreground, adjusted_rgb)`.
fn contrast_safe_accent(
    rgb: (u8, u8, u8),
    l: f64,
    c: f64,
    h: f64,
    bg_lum: f64,
) -> (bool, (u8, u8, u8)) {
    let base_lum = relative_luminance(rgb.0, rgb.1, rgb.2);

    let meets = |candidate_l: f64| -> bool {
        let lum = accent_luminance(candidate_l, c, h);
        contrast_ratio(lum, bg_lum) >= MIN_CONTRAST_RATIO
    };

    if meets(l) {
        return (select_accent_fg(base_lum), rgb);
    }

    // Push away from the surface: lighten if the accent is already lighter
    // than it, otherwise darken.
    let lighten = base_lum >= bg_lum;
    let (mut low, mut high) = if lighten { (l, 1.0) } else { (0.0, l) };
    for _ in 0..CONTRAST_SEARCH_ITERS {
        let mid = (low + high) / 2.0;
        if meets(mid) {
            if lighten {
                high = mid;
            } else {
                low = mid;
            }
        } else if lighten {
            low = mid;
        } else {
            high = mid;
        }
    }
    let adjusted_l = if lighten { high } else { low };
    let [r, g, b] = oklch_to_srgb_bytes(adjusted_l, c, h);
    (select_accent_fg(relative_luminance(r, g, b)), (r, g, b))
}

/// Pick the more contrasting of white and black for `accent-fg` at a given
/// luminance. `true` means white (the accent is dark enough for white text).
fn select_accent_fg(lum: f64) -> bool {
    contrast_ratio(1.0, lum) >= contrast_ratio(lum, 0.0)
}

/// The primary surface color for a theme mode, used for the `bg-primary` token
/// and as the reference surface the accent must keep contrast against.
fn bg_primary(is_dark: bool) -> (u8, u8, u8) {
    if is_dark {
        BG_PRIMARY_DARK
    } else {
        BG_PRIMARY_LIGHT
    }
}

/// Apply only the accent-derived tokens (accent, its foregrounds, and the
/// hover background) to a `Theme` global, leaving the rest of the palette
/// untouched. Used by the settings accent sliders so a live preview updates
/// just the accent instead of recomputing the whole theme on every tick.
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

/// Convert HSV (hue 0–360, s/v 0–1) to an sRGB color. Standard algorithm;
/// used to derive the accent color from the settings hue/saturation/brightness
/// tuning sliders.
pub(crate) fn hsv_to_rgb(h: f64, s: f64, v: f64) -> (u8, u8, u8) {
    let c = v * s;
    let x = c * (1.0 - ((h / 60.0) % 2.0 - 1.0).abs());
    let m = v - c;
    let (r, g, b) = match h as u32 % 360 {
        0..=59 => (c, x, 0.0),
        60..=119 => (x, c, 0.0),
        120..=179 => (0.0, c, x),
        180..=239 => (0.0, x, c),
        240..=299 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let to_u8 = |z: f64| ((z + m) * 255.0).round() as u8;
    (to_u8(r), to_u8(g), to_u8(b))
}

/// Convert sRGB (0–255) to HSV (hue 0–360, saturation 0–1, value 0–1).
/// Inverse of `hsv_to_rgb`; used to recover the selected accent's hue so the
/// settings tuning sliders can re-render it at a new saturation/brightness.
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

#[cfg(test)]
mod tests {
    use super::{contrast_ratio, contrast_safe_accent, relative_luminance, select_accent_fg};
    use cliptoo_core::color::srgb_bytes_to_oklch;

    const DARK_BG: (u8, u8, u8) = (0x18, 0x18, 0x18);
    const LIGHT_BG: (u8, u8, u8) = (0xF5, 0xF5, 0xF5);

    #[test]
    fn relative_luminance_reference_values() {
        // CSS Color 4 reference luminances.
        assert!((relative_luminance(0, 0, 0) - 0.0).abs() < 1e-9);
        assert!((relative_luminance(255, 255, 255) - 1.0).abs() < 1e-9);
        assert!((relative_luminance(255, 0, 0) - 0.2126).abs() < 1e-4);
        assert!((relative_luminance(0, 255, 0) - 0.7152).abs() < 1e-4);
        assert!((relative_luminance(0, 0, 255) - 0.0722).abs() < 1e-4);
    }

    #[test]
    fn contrast_ratio_is_symmetric() {
        assert!((contrast_ratio(1.0, 0.0) - 21.0).abs() < 1e-9);
        assert!((contrast_ratio(0.0, 1.0) - 21.0).abs() < 1e-9);
    }

    #[test]
    fn dark_accent_uses_white_foreground() {
        // Near-black navy: white text contrasts far better than black.
        assert!(select_accent_fg(relative_luminance(0x1A, 0x23, 0x7E)));
    }

    #[test]
    fn light_accent_uses_black_foreground() {
        // Bright yellow: black text contrasts far better than white.
        assert!(!select_accent_fg(relative_luminance(0xFF, 0xE6, 0x00)));
    }

    #[test]
    fn mid_tone_accent_uses_black_foreground() {
        // #808080 used to fall on the wrong side of the old Rec.601 threshold
        // (white text at ~4.1:1). By WCAG luminance it now picks black (~5.3:1).
        let lum = relative_luminance(0x80, 0x80, 0x80);
        assert!(!select_accent_fg(lum));
        assert!(contrast_ratio(lum, 0.0) >= 4.5);
    }

    fn adjusted(rgb: (u8, u8, u8), bg: (u8, u8, u8)) -> (bool, (u8, u8, u8)) {
        let (l, c, h) = srgb_bytes_to_oklch(rgb.0, rgb.1, rgb.2);
        let bg_lum = relative_luminance(bg.0, bg.1, bg.2);
        contrast_safe_accent(rgb, l, c, h, bg_lum)
    }

    #[test]
    fn adjusted_accent_contrasts_with_dark_surface() {
        for rgb in [
            (0x7C, 0x6E, 0xE6),
            (0x1A, 0x23, 0x7E),
            (0xFF, 0xE6, 0x00),
            (0x80, 0x80, 0x80),
        ] {
            let (_, out) = adjusted(rgb, DARK_BG);
            let lum = relative_luminance(out.0, out.1, out.2);
            let bg_lum = relative_luminance(DARK_BG.0, DARK_BG.1, DARK_BG.2);
            assert!(
                contrast_ratio(lum, bg_lum) >= super::MIN_CONTRAST_RATIO,
                "rgb {rgb:?} on dark: contrast {}",
                contrast_ratio(lum, bg_lum)
            );
        }
    }

    #[test]
    fn adjusted_accent_contrasts_with_light_surface() {
        for rgb in [
            (0x7C, 0x6E, 0xE6),
            (0x1A, 0x23, 0x7E),
            (0xFF, 0xE6, 0x00),
            (0x80, 0x80, 0x80),
        ] {
            let (_, out) = adjusted(rgb, LIGHT_BG);
            let lum = relative_luminance(out.0, out.1, out.2);
            let bg_lum = relative_luminance(LIGHT_BG.0, LIGHT_BG.1, LIGHT_BG.2);
            assert!(
                contrast_ratio(lum, bg_lum) >= super::MIN_CONTRAST_RATIO,
                "rgb {rgb:?} on light: contrast {}",
                contrast_ratio(lum, bg_lum)
            );
        }
    }

    #[test]
    fn dark_accent_lightens_and_light_accent_darkens() {
        // Navy is unreadable on the dark surface: it must lighten.
        let (_, out) = adjusted((0x1A, 0x23, 0x7E), DARK_BG);
        assert!(relative_luminance(out.0, out.1, out.2) > relative_luminance(0x1A, 0x23, 0x7E));

        // Bright yellow is unreadable on the light surface: it must darken.
        let (_, out) = adjusted((0xFF, 0xE6, 0x00), LIGHT_BG);
        assert!(relative_luminance(out.0, out.1, out.2) < relative_luminance(0xFF, 0xE6, 0x00));
    }

    #[test]
    fn hue_is_preserved_through_adjustment() {
        let bg_lum = relative_luminance(DARK_BG.0, DARK_BG.1, DARK_BG.2);
        let rgb = (0x1A, 0x23, 0x7E);
        let (l, c, h) = srgb_bytes_to_oklch(rgb.0, rgb.1, rgb.2);
        let (_, out) = contrast_safe_accent(rgb, l, c, h, bg_lum);
        let (_, _, out_h) = srgb_bytes_to_oklch(out.0, out.1, out.2);
        let delta = (out_h - h).abs().min(360.0 - (out_h - h).abs());
        assert!(delta < 5.0, "hue drifted {h} -> {out_h}");
    }
}
