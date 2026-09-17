//! WCAG contrast math and OKLCH-lightness adjustment for accent colors.

use cliptoo_core::color::oklch_to_srgb_bytes;
use slint::Color;

/// Primary surface the accent is applied on (background). The accent must
/// keep a minimum contrast against this in both theme modes.
const BG_PRIMARY_DARK: (u8, u8, u8) = (0x18, 0x18, 0x18);
const BG_PRIMARY_LIGHT: (u8, u8, u8) = (0xF5, 0xF5, 0xF5);

/// Minimum WCAG contrast ratio (AA, normal text) required between the accent
/// and the surface it is applied to.
const MIN_CONTRAST_RATIO: f64 = 4.5;

/// Binary-search iterations for the OKLCH-lightness contrast adjustment.
/// Precision is bounded by the lightness range (0–1) divided by 2^iters.
const CONTRAST_SEARCH_ITERS: u32 = 32;

/// Derive an accent-tinted color at a fixed OKLCH lightness `l`, scaling the
/// base accent's chroma by `chroma_scale`. Used for the subtle accent border,
/// so a picked Neon stays vibrant and a picked Muted stays restrained.
pub(super) fn accent_sibling(hue: f64, base_chroma: f64, l: f64, chroma_scale: f64) -> Color {
    let [r, g, b] = oklch_to_srgb_bytes(l, base_chroma * chroma_scale, hue);
    Color::from_rgb_u8(r, g, b)
}

/// WCAG 2.x relative luminance (0–1) of an sRGB color.
pub(super) fn relative_luminance(r: u8, g: u8, b: u8) -> f64 {
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
pub(super) fn contrast_safe_accent(
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
pub(super) fn select_accent_fg(lum: f64) -> bool {
    contrast_ratio(1.0, lum) >= contrast_ratio(lum, 0.0)
}

/// The primary surface color for a theme mode, used for the `bg-primary` token
/// and as the reference surface the accent must keep contrast against.
pub(super) fn bg_primary(is_dark: bool) -> (u8, u8, u8) {
    if is_dark {
        BG_PRIMARY_DARK
    } else {
        BG_PRIMARY_LIGHT
    }
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
