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
    let conn = zbus::Connection::session().await.ok()?;
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
    if let Ok(conn) = zbus::Connection::session().await
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

fn blend(base: Color, accent: Color, alpha: f32) -> Color {
    let r = (alpha * accent.red() as f32 + (1.0 - alpha) * base.red() as f32) as u8;
    let g = (alpha * accent.green() as f32 + (1.0 - alpha) * base.green() as f32) as u8;
    let b = (alpha * accent.blue() as f32 + (1.0 - alpha) * base.blue() as f32) as u8;
    Color::from_rgb_u8(r, g, b)
}

/// Fallback accent RGB used when no valid hex is stored (core settings
/// default is `#7C6EE6`).
const DEFAULT_ACCENT: (u8, u8, u8) = (0x7C, 0x6E, 0xE6);

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

/// Derive a lighter/dimmer sibling of the base accent at OKLCH lightness `l`,
/// scaling the base color's own chroma by `chroma_scale` — a picked Neon stays
/// vibrant, a picked Muted stays restrained.
fn accent_sibling(hue: f64, base_chroma: f64, l: f64, chroma_scale: f64) -> Color {
    let [r, g, b] = oklch_to_srgb_bytes(l, base_chroma * chroma_scale, hue);
    Color::from_rgb_u8(r, g, b)
}

/// Apply pre-resolved theme tokens to any `Theme` global handle.
/// Used by the window, the tray, and any other component with `Theme`.
pub fn fill_theme(
    t: &Theme,
    settings: &Settings,
    is_dark: bool,
    system_accent: Option<(u8, u8, u8)>,
) {
    let (secondary_l, muted_l) = if is_dark { (0.58, 0.54) } else { (0.44, 0.40) };

    // Base accent: the system accent in "System" theme mode, otherwise the
    // user-picked color. Secondary/muted shades are derived from whichever was
    // used, keeping hue and relative chroma consistent.
    let (accent, hue, base_chroma) = if let Some((sr, sg, sb)) = system_accent {
        tracing::debug!("detected system accent: #{sr:02X}{sg:02X}{sb:02X}");
        let (_, sys_c, sys_h) = srgb_bytes_to_oklch(sr, sg, sb);
        (Color::from_rgb_u8(sr, sg, sb), sys_h, sys_c)
    } else {
        let (r, g, b) = parse_accent_hex(&settings.accent_color);
        let (_, c, h) = srgb_bytes_to_oklch(r, g, b);
        (Color::from_rgb_u8(r, g, b), h, c)
    };

    let ar = accent.red() as f32;
    let ag = accent.green() as f32;
    let ab = accent.blue() as f32;
    t.set_accent_is_dark((0.299 * ar + 0.587 * ag + 0.114 * ab) <= 128.0);
    t.set_accent_primary(accent);
    t.set_accent_secondary(accent_sibling(hue, base_chroma, secondary_l, 0.75));
    t.set_accent_muted(accent_sibling(hue, base_chroma, muted_l, 0.40));

    if is_dark {
        t.set_bg_primary(Color::from_rgb_u8(0x18, 0x18, 0x18));
        t.set_bg_header(Color::from_rgb_u8(0x12, 0x12, 0x12));
        t.set_bg_row_alt(Color::from_rgb_u8(0x1C, 0x1C, 0x1C));
        t.set_bg_row_hover(blend(Color::from_rgb_u8(0x1C, 0x1C, 0x1C), accent, 0.30));
        t.set_bg_row_selected(Color::from_rgb_u8(0x28, 0x28, 0x28));
        t.set_bg_input(Color::from_rgb_u8(0x24, 0x24, 0x24));
        t.set_fg_primary(Color::from_rgb_u8(0xE4, 0xE4, 0xE4));
        t.set_fg_secondary(Color::from_rgb_u8(0xA1, 0xA1, 0xA1));
        t.set_fg_muted(Color::from_rgb_u8(0x63, 0x63, 0x63));
        t.set_fg_danger(Color::from_rgb_u8(0xB0, 0xB0, 0xB0));
        t.set_fg_success(Color::from_rgb_u8(0x90, 0x90, 0x90));
        t.set_fg_warning(Color::from_rgb_u8(0xA0, 0xA0, 0xA0));
        t.set_fg_bookmarked(Color::from_rgb_u8(0xC0, 0xC0, 0xC0));
        t.set_border_subtle(Color::from_rgb_u8(0x44, 0x44, 0x44));
        t.set_shadow(Color::from_rgb_u8(0x00, 0x00, 0x00));
    } else {
        t.set_bg_primary(Color::from_rgb_u8(0xF5, 0xF5, 0xF5));
        t.set_bg_header(Color::from_rgb_u8(0xE8, 0xE8, 0xE8));
        t.set_bg_row_alt(Color::from_rgb_u8(0xEE, 0xEE, 0xEE));
        t.set_bg_row_hover(blend(Color::from_rgb_u8(0xEE, 0xEE, 0xEE), accent, 0.25));
        t.set_bg_row_selected(Color::from_rgb_u8(0xDD, 0xDD, 0xDD));
        t.set_bg_input(Color::from_rgb_u8(0xFF, 0xFF, 0xFF));
        t.set_fg_primary(Color::from_rgb_u8(0x1C, 0x1C, 0x1C));
        t.set_fg_secondary(Color::from_rgb_u8(0x55, 0x55, 0x55));
        t.set_fg_muted(Color::from_rgb_u8(0x99, 0x99, 0x99));
        t.set_fg_danger(Color::from_rgb_u8(0xAA, 0xAA, 0xAA));
        t.set_fg_success(Color::from_rgb_u8(0x88, 0x88, 0x88));
        t.set_fg_warning(Color::from_rgb_u8(0x99, 0x99, 0x99));
        t.set_fg_bookmarked(Color::from_rgb_u8(0xBB, 0xBB, 0xBB));
        t.set_border_subtle(Color::from_rgb_u8(0xD0, 0xD0, 0xD0));
        t.set_shadow(Color::from_rgb_u8(0x00, 0x00, 0x00));
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
    if let Ok(mut cached) = RESOLVED_THEME.lock() {
        *cached = Some((is_dark, system_accent));
    }
}

/// The last resolved theme context, or `(dark, no accent)` if never resolved.
pub fn cached_resolved_theme() -> ResolvedTheme {
    RESOLVED_THEME
        .lock()
        .ok()
        .and_then(|guard| *guard)
        .unwrap_or((true, None))
}

/// Convert HSV (hue 0–360, s/v 0–1) to an sRGB color. Standard algorithm;
/// used to build the evenly-hue-spaced accent swatch palette.
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

/// Resolve the dark-mode and system-accent context for the given settings.
/// Side-effect: caches the result for `cached_resolved_theme`.
pub async fn resolve_theme(settings: &Settings) -> ResolvedTheme {
    let is_dark = match settings.theme.as_str() {
        "Light" => false,
        "Dark" => true,
        _ => detect_system_dark().await.unwrap_or(true),
    };
    // Follow the OS accent in "System" theme mode, and always when the accent
    // has been cleared (empty string) — that is the "use OS default" state.
    let system_accent = if settings.accent_color.trim().is_empty()
        || (settings.theme.as_str() != "Light" && settings.theme.as_str() != "Dark")
    {
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
