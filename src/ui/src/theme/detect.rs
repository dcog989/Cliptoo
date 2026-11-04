//! System theme detection: the dark/light preference and accent color are read
//! from the XDG Desktop Portal, with a KDE Plasma 6 `kdeglobals` fallback.

use std::time::Duration;
use zbus::zvariant::{OwnedValue, Value};

/// A portal that owns its bus name but never replies must not hang startup:
/// the theme is resolved before the main window is shown.
const PORTAL_TIMEOUT: Duration = Duration::from_secs(2);

/// Read one `org.freedesktop.appearance` value from the portal.
///
/// The `Settings.Read` reply is a variant that contains the value, and the
/// value arrives wrapped once more (`v v u 1` on the wire). zbus will not
/// deserialize the outer variant straight into the payload type, so take the
/// inner `Value` first and return that in an owned form.
async fn read_appearance(key: &str) -> Option<OwnedValue> {
    let conn = crate::dbus::session().await.ok()?;
    let msg = tokio::time::timeout(
        PORTAL_TIMEOUT,
        conn.call_method(
            Some("org.freedesktop.portal.Desktop"),
            "/org/freedesktop/portal/desktop",
            Some("org.freedesktop.portal.Settings"),
            "Read",
            &("org.freedesktop.appearance", key),
        ),
    )
    .await
    .ok()?
    .ok()?;

    let raw: OwnedValue = msg.body().deserialize().ok()?;
    raw.downcast_ref::<Value>().ok()?.try_to_owned().ok()
}

/// Detect the system color-scheme preference via xdg-desktop-portal.
/// Returns `true` for dark, `false` for light, `None` if undetectable.
pub async fn detect_system_dark() -> Option<bool> {
    let val = u32::try_from(read_appearance("color-scheme").await?).ok()?;
    match val {
        1 => Some(true),
        2 => Some(false),
        _ => None,
    }
}

/// Whether the session is KDE Plasma, where `kdeglobals` holds the user's
/// picked accent while the portal reports the derived palette highlight.
fn on_kde() -> bool {
    std::env::var("XDG_CURRENT_DESKTOP").is_ok_and(|desktops| {
        desktops
            .split(':')
            .any(|name| name.eq_ignore_ascii_case("KDE"))
    })
}

/// Read `key` from `[section]` in `$XDG_CONFIG_HOME/kdeglobals`.
fn read_kdeglobals_value(section: &str, key: &str) -> Option<String> {
    let path = dirs::config_dir()?.join("kdeglobals");
    let text = std::fs::read_to_string(path).ok()?;
    let header = format!("[{section}]");
    let mut in_section = false;
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') {
            in_section = trimmed == header;
            continue;
        }
        if in_section
            && let Some(value) = trimmed.strip_prefix(key)
            && let Some(value) = value.strip_prefix('=')
        {
            return Some(value.trim().to_string());
        }
    }
    None
}

/// Whether the desktop asks for animated transitions. KDE Plasma stores
/// `AnimationDurationFactor=0` under `[KDE]` when animations are disabled; an
/// unreadable file keeps the app's default animations on.
pub fn animations_enabled() -> bool {
    static ENABLED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ENABLED.get_or_init(|| {
        read_kdeglobals_value("KDE", "AnimationDurationFactor")
            .and_then(|v| v.parse::<f64>().ok())
            .is_none_or(|factor| factor > 0.0)
    })
}

/// The system UI font size in logical pixels, derived from Plasma's
/// `[General] font` entry (`Family,PointSize,…`). Qt reports points; Slint
/// works in pixels at 96 dpi. `None` when the entry is missing or malformed,
/// so callers keep their fallback.
pub fn system_font_size_px() -> Option<f32> {
    static SIZE: std::sync::OnceLock<Option<f32>> = std::sync::OnceLock::new();
    *SIZE.get_or_init(|| {
        let raw = read_kdeglobals_value("General", "font")?;
        let points: f32 = raw.split(',').nth(1)?.trim().parse().ok()?;
        // 1 pt = 96/72 px at the 96 dpi Slint assumes.
        Some((points * 96.0 / 72.0).round())
    })
}

/// Try to read the KDE Plasma 6 accent color from `$XDG_CONFIG_HOME/kdeglobals`.
/// Plasma 6 writes the user's chosen accent color as `AccentColor=r,g,b,a`
/// under `[General]` in this file.
fn read_kdeglobals_accent() -> Option<(u8, u8, u8)> {
    let raw = read_kdeglobals_value("General", "AccentColor")?;
    let parts: Vec<&str> = raw.split(',').collect();
    if parts.len() < 3 {
        return None;
    }
    let r = parts[0].trim().parse::<u8>().ok()?;
    let g = parts[1].trim().parse::<u8>().ok()?;
    let b = parts[2].trim().parse::<u8>().ok()?;
    Some((r, g, b))
}

/// Detect the system accent color. On KDE Plasma the portal's
/// `accent-color` is `QPalette::highlight()` — the accent blended toward the
/// window background — so the stored `AccentColor` is read first there.
/// Elsewhere the XDG portal is authoritative, with `kdeglobals` as a last
/// resort when the portal call fails.
/// Returns `(r, g, b)` in 0–255, or `None` if undetectable.
pub async fn detect_system_accent() -> Option<(u8, u8, u8)> {
    let accent = read_system_accent().await;
    // Logged here, once per detection: the token fillers run for every window.
    if let Some((r, g, b)) = accent {
        tracing::debug!("detected system accent: #{r:02X}{g:02X}{b:02X}");
    }
    accent
}

/// One detection attempt; see [`detect_system_accent`].
async fn read_system_accent() -> Option<(u8, u8, u8)> {
    if on_kde()
        && let Some(rgb) = read_kdeglobals_accent()
    {
        return Some(rgb);
    }
    // Try the portal (org.freedesktop.appearance.accent-color)
    if let Some(value) = read_appearance("accent-color").await
        && let Ok((r, g, b)) = <(f64, f64, f64)>::try_from(&*value)
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
