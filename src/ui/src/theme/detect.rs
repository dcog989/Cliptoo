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

/// Try to read the KDE Plasma 6 accent color from `$XDG_CONFIG_HOME/kdeglobals`.
/// Plasma 6 writes the user's chosen accent color as `AccentColor=r,g,b,a`
/// under `[General]` in this file.
fn read_kdeglobals_accent() -> Option<(u8, u8, u8)> {
    let path = dirs::config_dir()?.join("kdeglobals");

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

/// Detect the system accent color. On KDE Plasma the portal's
/// `accent-color` is `QPalette::highlight()` — the accent blended toward the
/// window background — so the stored `AccentColor` is read first there.
/// Elsewhere the XDG portal is authoritative, with `kdeglobals` as a last
/// resort when the portal call fails.
/// Returns `(r, g, b)` in 0–255, or `None` if undetectable.
pub async fn detect_system_accent() -> Option<(u8, u8, u8)> {
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
