use crate::AppWindow;
use crate::Theme;
use cliptoo_core::Settings;
use cliptoo_core::color::{
    chroma_level_factor, find_max_chroma, oklch_to_srgb_bytes, srgb_bytes_to_oklch,
};
use slint::{Color, ComponentHandle, SharedString};

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

fn accent_color(h: f64, l: f64, chroma_scale: f64, chroma_level: f64) -> Color {
    let max_c = find_max_chroma(l, h);
    let c = max_c * chroma_level * chroma_scale;
    let [r, g, b] = oklch_to_srgb_bytes(l, c, h);
    Color::from_rgb_u8(r, g, b)
}

/// Apply theme tokens to the Slint UI global `Theme`.
pub async fn apply_theme(ui: &AppWindow, settings: &Settings) {
    let is_dark = match settings.theme.as_str() {
        "Light" => false,
        "Dark" => true,
        _ => detect_system_dark().await.unwrap_or(true),
    };
    let system_accent = if settings.theme.as_str() != "Light" && settings.theme.as_str() != "Dark" {
        detect_system_accent().await
    } else {
        None
    };
    apply_theme_inner(ui, settings, is_dark, system_accent);
}

/// Apply theme tokens synchronously, with pre-resolved dark/accent values.
/// Used by `apply_theme` (with portal I/O) and by `reapply_theme` in
/// `settings.rs` (from a Qt callback where async spawning is not possible).
pub fn apply_theme_inner(
    ui: &AppWindow,
    settings: &Settings,
    is_dark: bool,
    system_accent: Option<(u8, u8, u8)>,
) {
    let t = ui.global::<Theme>();

    let (base_l, secondary_l, muted_l) = if is_dark {
        (0.62, 0.58, 0.54)
    } else {
        (0.48, 0.44, 0.40)
    };

    let chroma_level = chroma_level_factor(&settings.accent_chroma_level);

    let (accent, hue) = if let Some((sr, sg, sb)) = system_accent {
        tracing::debug!("detected system accent: #{sr:02X}{sg:02X}{sb:02X}");
        let (_, _, sys_h) = srgb_bytes_to_oklch(sr, sg, sb);
        (Color::from_rgb_u8(sr, sg, sb), sys_h)
    } else {
        let a = accent_color(settings.accent_hue, base_l, 1.0, chroma_level);
        (a, settings.accent_hue)
    };

    let ar = accent.red() as f32;
    let ag = accent.green() as f32;
    let ab = accent.blue() as f32;
    t.set_accent_is_dark((0.299 * ar + 0.587 * ag + 0.114 * ab) <= 128.0);
    t.set_accent_primary(accent);
    t.set_accent_secondary(accent_color(hue, secondary_l, 0.75, chroma_level));
    t.set_accent_muted(accent_color(hue, muted_l, 0.40, chroma_level));

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
    t.set_font_size(settings.font_size as f32);
    t.set_preview_font_size(settings.preview_font_size as f32);

    t.set_hover_preview_delay(settings.hover_preview_delay as i64);

    t.set_row_height(crate::positioning::row_height(&settings.clip_item_padding) as f32);
}
