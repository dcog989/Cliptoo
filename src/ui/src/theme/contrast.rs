//! APCA-W3 perceptual contrast and OKLCH-lightness adjustment for accent colors.
//!
//! Contrast is measured with [APCA-W3] (0.1.9 / 0.98G-4g), not the WCAG 2.x
//! ratio: APCA is polarity-aware and perceptual, which matters for text drawn
//! on colored surfaces and for small inline highlights where a luminance-ratio
//! threshold is a poor proxy for legibility. The algorithm is ported inline
//! (no dependency) from the Myndex reference implementation.
//!
//! [APCA-W3]: https://github.com/Myndex/SAPC-APCA

use cliptoo_core::color::oklch_to_srgb_bytes;
use slint::Color;

/// Primary surface the accent is applied on (background). The accent must
/// keep a minimum APCA contrast against this in both theme modes.
const BG_PRIMARY_DARK: (u8, u8, u8) = (0x18, 0x18, 0x18);
const BG_PRIMARY_LIGHT: (u8, u8, u8) = (0xF5, 0xF5, 0xF5);

/// APCA floor for accent used as text or a thin stroke (APCA's "any text /
/// large or bold" floor).
pub(super) const TEXT_MIN_LC: f64 = 60.0;

/// APCA floor for accent used as a filled shape. Lower because a shape only
/// has to read as a shape — the text floor turns every fill into a pastel.
pub(super) const FILL_MIN_LC: f64 = 30.0;

/// Binary-search iterations for the OKLCH-lightness contrast adjustment.
/// Precision is bounded by the lightness range (0–1) divided by 2^iters.
const CONTRAST_SEARCH_ITERS: u32 = 32;

/// APCA Lc (magnitude) the FTS search-match highlight must keep against the
/// row surface. Lc 75 is APCA's fluent-body-text floor; a match is a few
/// characters embedded in a line of body text and must read clearly.
const MATCH_HIGHLIGHT_LC: f64 = 75.0;

/// Fixed OKLCH chroma for the search-match highlight. Taken as a floor rather
/// than the accent's own chroma so a muted or near-grey accent still yields a
/// visibly chromatic match instead of one that reads as body text.
const MATCH_HIGHLIGHT_CHROMA: f64 = 0.16;

/// Seed OKLCH lightness for the highlight search. Mid-lightness keeps the
/// strong fixed chroma inside the sRGB gamut for every hue; the search only
/// moves off it when contrast against the surface requires it.
const MATCH_HIGHLIGHT_SEED_L: f64 = 0.5;

const WHITE: (u8, u8, u8) = (0xFF, 0xFF, 0xFF);
const BLACK: (u8, u8, u8) = (0x00, 0x00, 0x00);

// ── APCA-W3 (0.1.9 / 0.98G-4g) constants ────────────────────────────────────
// Names mirror the reference implementation (SA98G) for easy cross-checking.
const APCA_MAIN_TRC: f64 = 2.4;
const APCA_R_CO: f64 = 0.2126729;
const APCA_G_CO: f64 = 0.7151522;
const APCA_B_CO: f64 = 0.0721750;
const APCA_NORM_BG: f64 = 0.56;
const APCA_NORM_TXT: f64 = 0.57;
const APCA_REV_TXT: f64 = 0.62;
const APCA_REV_BG: f64 = 0.65;
const APCA_BLK_THRS: f64 = 0.022;
const APCA_BLK_CLMP: f64 = 1.414;
const APCA_SCALE_BOW: f64 = 1.14;
const APCA_SCALE_WOB: f64 = 1.14;
const APCA_LO_BOW_OFFSET: f64 = 0.027;
const APCA_LO_WOB_OFFSET: f64 = 0.027;
const APCA_DELTA_Y_MIN: f64 = 0.0005;
const APCA_LO_CLIP: f64 = 0.1;

/// APCA luma: the sRGB channels raised to the 2.4 transfer curve and weighted.
/// This is the `Y` that [`apca_contrast`] consumes, not WCAG relative
/// luminance (which uses the piecewise sRGB curve over a different model).
fn apca_luma(r: u8, g: u8, b: u8) -> f64 {
    fn channel(c: u8) -> f64 {
        (c as f64 / 255.0).powf(APCA_MAIN_TRC)
    }
    APCA_R_CO * channel(r) + APCA_G_CO * channel(g) + APCA_B_CO * channel(b)
}

/// APCA soft-clamps near-black luma so very dark colours do not collapse to a
/// single value (the reference's `blkThrs` / `blkClmp` step).
fn clamp_black(y: f64) -> f64 {
    if y > APCA_BLK_THRS {
        y
    } else {
        y + (APCA_BLK_THRS - y).powf(APCA_BLK_CLMP)
    }
}

/// Signed APCA lightness contrast `Lc` between a text and a background color,
/// in roughly −108…+106. Positive is normal polarity (dark text on a light
/// background); negative is reverse polarity (light text on a dark one). It is
/// the magnitude that rates legibility; `0.0` means the pair is below the
/// algorithm's usable floor.
pub(super) fn apca_contrast(text: (u8, u8, u8), bg: (u8, u8, u8)) -> f64 {
    let txt_y = clamp_black(apca_luma(text.0, text.1, text.2));
    let bg_y = clamp_black(apca_luma(bg.0, bg.1, bg.2));

    if (bg_y - txt_y).abs() < APCA_DELTA_Y_MIN {
        return 0.0;
    }

    let sapc = if bg_y > txt_y {
        (bg_y.powf(APCA_NORM_BG) - txt_y.powf(APCA_NORM_TXT)) * APCA_SCALE_BOW
    } else {
        (bg_y.powf(APCA_REV_BG) - txt_y.powf(APCA_REV_TXT)) * APCA_SCALE_WOB
    };

    let output = if bg_y > txt_y {
        if sapc < APCA_LO_CLIP {
            0.0
        } else {
            sapc - APCA_LO_BOW_OFFSET
        }
    } else if sapc > -APCA_LO_CLIP {
        0.0
    } else {
        sapc + APCA_LO_WOB_OFFSET
    };

    output * 100.0
}

/// The accent's hue at OKLCH lightness `l`, at the given chroma, so a picked
/// Neon stays vibrant and a picked Muted stays restrained. Used for the accent
/// border and the hovered-row surface.
pub(super) fn accent_sibling(hue: f64, chroma: f64, l: f64) -> Color {
    let [r, g, b] = oklch_to_srgb_bytes(l, chroma, hue);
    Color::from_rgb_u8(r, g, b)
}

/// Adjust the accent's OKLCH lightness (hue and chroma held fixed) so it keeps
/// `min_lc` APCA contrast against the surface it is applied to. The accent is
/// pushed away from the surface's luma — lightened on dark surfaces, darkened
/// on light ones — so its hue and saturation survive.
///
/// Returns `(use_white_foreground, adjusted_rgb)`.
pub(super) fn contrast_safe_accent(
    rgb: (u8, u8, u8),
    l: f64,
    c: f64,
    h: f64,
    bg: (u8, u8, u8),
    min_lc: f64,
) -> (bool, (u8, u8, u8)) {
    let base_y = apca_luma(rgb.0, rgb.1, rgb.2);
    match contrast_safe_lightness(base_y, l, c, h, bg, min_lc) {
        None => (select_accent_fg(rgb), rgb),
        Some(adjusted_l) => {
            let [r, g, b] = oklch_to_srgb_bytes(adjusted_l, c, h);
            (select_accent_fg((r, g, b)), (r, g, b))
        }
    }
}

/// The OKLCH lightness (hue and chroma held fixed) whose APCA contrast against
/// `bg` first reaches `min_lc`, moving away from the surface — up when
/// `base_y` is already above it, down otherwise. Returns `None` when `l`
/// already meets the target, so callers keep the original colour unchanged.
fn contrast_safe_lightness(
    base_y: f64,
    l: f64,
    c: f64,
    h: f64,
    bg: (u8, u8, u8),
    min_lc: f64,
) -> Option<f64> {
    let bg_y = apca_luma(bg.0, bg.1, bg.2);
    let meets = |candidate_l: f64| -> bool {
        let [r, g, b] = oklch_to_srgb_bytes(candidate_l, c, h);
        apca_contrast((r, g, b), bg).abs() >= min_lc
    };

    if meets(l) {
        return None;
    }

    let lighten = base_y >= bg_y;
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
    Some(if lighten { high } else { low })
}

/// Build the FTS search-match highlight: the accent's hue rendered at a fixed
/// strong chroma ([`MATCH_HIGHLIGHT_CHROMA`]) with a high minimum APCA
/// contrast ([`MATCH_HIGHLIGHT_LC`]) against the row surface.
///
/// Unlike [`contrast_safe_accent`], the chroma is not inherited from the accent
/// — a muted or near-grey pick would otherwise leave the highlight looking like
/// body text. Only the hue is calibrated from the accent, so the match stays
/// on-theme while standing out.
pub(super) fn match_highlight(hue: f64, bg: (u8, u8, u8)) -> Color {
    let l = MATCH_HIGHLIGHT_SEED_L;
    let [r, g, b] = oklch_to_srgb_bytes(l, MATCH_HIGHLIGHT_CHROMA, hue);
    let base_y = apca_luma(r, g, b);
    let adjusted_l = contrast_safe_lightness(
        base_y,
        l,
        MATCH_HIGHLIGHT_CHROMA,
        hue,
        bg,
        MATCH_HIGHLIGHT_LC,
    )
    .unwrap_or(l);
    let [r, g, b] = oklch_to_srgb_bytes(adjusted_l, MATCH_HIGHLIGHT_CHROMA, hue);
    Color::from_rgb_u8(r, g, b)
}

/// Pick the more legible of white and black for text drawn on `rgb`, by APCA
/// magnitude. `true` means white (the accent is dark enough for white text).
pub(super) fn select_accent_fg(rgb: (u8, u8, u8)) -> bool {
    apca_contrast(WHITE, rgb).abs() >= apca_contrast(BLACK, rgb).abs()
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
    use super::{
        BLACK, FILL_MIN_LC, MATCH_HIGHLIGHT_LC, TEXT_MIN_LC, WHITE, apca_contrast, apca_luma,
        contrast_safe_accent, match_highlight, select_accent_fg,
    };
    use cliptoo_core::color::srgb_bytes_to_oklch;

    const DARK_BG: (u8, u8, u8) = (0x18, 0x18, 0x18);
    const LIGHT_BG: (u8, u8, u8) = (0xF5, 0xF5, 0xF5);

    /// Chroma floor asserted for the highlight. Lower than
    /// [`MATCH_HIGHLIGHT_CHROMA`] because the sRGB gamut, not the requested
    /// chroma, is the binding constraint once [`MATCH_HIGHLIGHT_LC`] forces a
    /// light colour: hue 285 on [`DARK_BG`] lands at L* 0.852, where the gamut
    /// tops out at 0.076 — and the highlight reaches exactly that.
    const MIN_MATCH_HIGHLIGHT_CHROMA: f64 = 0.07;

    #[test]
    fn apca_reference_values() {
        // Published APCA-W3 (0.98G-4g) test pairs from the Myndex reference.
        assert!((apca_contrast(BLACK, WHITE) - 106.04).abs() < 0.2);
        assert!((apca_contrast(WHITE, BLACK) + 107.88).abs() < 0.2);
        assert!((apca_contrast((0x88, 0x88, 0x88), WHITE) - 63.06).abs() < 0.2);
        assert!((apca_contrast(WHITE, (0x88, 0x88, 0x88)) + 68.54).abs() < 0.2);
    }

    #[test]
    fn dark_accent_uses_white_foreground() {
        // Near-black navy: white text contrasts far better than black.
        assert!(select_accent_fg((0x1A, 0x23, 0x7E)));
    }

    #[test]
    fn light_accent_uses_black_foreground() {
        // Bright yellow: black text contrasts far better than white.
        assert!(!select_accent_fg((0xFF, 0xE6, 0x00)));
    }

    #[test]
    fn mid_tone_accent_uses_white_foreground() {
        // APCA rates white on #808080 (|Lc| ≈ 72) above black (≈ 37), the
        // opposite of the WCAG luminance crossover the theme used before.
        assert!(select_accent_fg((0x80, 0x80, 0x80)));
    }

    fn adjusted(rgb: (u8, u8, u8), bg: (u8, u8, u8), min_lc: f64) -> (bool, (u8, u8, u8)) {
        let (l, c, h) = srgb_bytes_to_oklch(rgb.0, rgb.1, rgb.2);
        contrast_safe_accent(rgb, l, c, h, bg, min_lc)
    }

    #[test]
    fn adjusted_accent_meets_min_lc_on_dark_surface() {
        for rgb in [
            (0x7C, 0x6E, 0xE6),
            (0x1A, 0x23, 0x7E),
            (0xFF, 0xE6, 0x00),
            (0x80, 0x80, 0x80),
        ] {
            let (_, out) = adjusted(rgb, DARK_BG, TEXT_MIN_LC);
            let lc = apca_contrast(out, DARK_BG).abs();
            assert!(lc >= TEXT_MIN_LC - 0.1, "rgb {rgb:?} on dark: Lc {lc}");
        }
    }

    #[test]
    fn adjusted_accent_meets_min_lc_on_light_surface() {
        for rgb in [
            (0x7C, 0x6E, 0xE6),
            (0x1A, 0x23, 0x7E),
            (0xFF, 0xE6, 0x00),
            (0x80, 0x80, 0x80),
        ] {
            let (_, out) = adjusted(rgb, LIGHT_BG, TEXT_MIN_LC);
            let lc = apca_contrast(out, LIGHT_BG).abs();
            assert!(lc >= TEXT_MIN_LC - 0.1, "rgb {rgb:?} on light: Lc {lc}");
        }
    }

    #[test]
    fn fill_accent_meets_min_lc_on_both_surfaces() {
        for bg in [DARK_BG, LIGHT_BG] {
            for rgb in [
                (0x7C, 0x6E, 0xE6),
                (0x1A, 0x23, 0x7E),
                (0xFF, 0xE6, 0x00),
                (0x80, 0x80, 0x80),
            ] {
                let (_, out) = adjusted(rgb, bg, FILL_MIN_LC);
                let lc = apca_contrast(out, bg).abs();
                assert!(lc >= FILL_MIN_LC - 0.1, "rgb {rgb:?} on {bg:?}: Lc {lc}");
            }
        }
    }

    #[test]
    fn fill_accent_keeps_the_picked_color() {
        // The point of the lower fill floor: an accent that is already visible
        // on the page must survive as a filled shape instead of being lightened
        // into a pastel (which is what the text-grade floor did).
        for rgb in [(0x7C, 0x6E, 0xE6), (0xDA, 0x44, 0x53), (0x1D, 0x99, 0xF3)] {
            let (_, out) = adjusted(rgb, DARK_BG, FILL_MIN_LC);
            assert_eq!(out, rgb, "fill accent shifted for {rgb:?}");
        }
    }

    #[test]
    fn dark_accent_lightens_and_light_accent_darkens() {
        // Navy is unreadable on the dark surface: it must lighten.
        let (_, out) = adjusted((0x1A, 0x23, 0x7E), DARK_BG, TEXT_MIN_LC);
        assert!(apca_luma(out.0, out.1, out.2) > apca_luma(0x1A, 0x23, 0x7E));

        // Bright yellow is unreadable on the light surface: it must darken.
        let (_, out) = adjusted((0xFF, 0xE6, 0x00), LIGHT_BG, TEXT_MIN_LC);
        assert!(apca_luma(out.0, out.1, out.2) < apca_luma(0xFF, 0xE6, 0x00));
    }

    #[test]
    fn hue_is_preserved_through_adjustment() {
        let rgb = (0x1A, 0x23, 0x7E);
        let (l, c, h) = srgb_bytes_to_oklch(rgb.0, rgb.1, rgb.2);
        let (_, out) = contrast_safe_accent(rgb, l, c, h, DARK_BG, TEXT_MIN_LC);
        let (_, _, out_h) = srgb_bytes_to_oklch(out.0, out.1, out.2);
        let delta = (out_h - h).abs().min(360.0 - (out_h - h).abs());
        assert!(delta < 5.0, "hue drifted {h} -> {out_h}");
    }

    fn bytes(c: super::Color) -> (u8, u8, u8) {
        (c.red(), c.green(), c.blue())
    }

    #[test]
    fn match_highlight_keeps_strong_chroma() {
        // A near-grey accent hue still yields a chromatic highlight: the fixed
        // chroma floor is what separates the match from achromatic body text.
        for bg in [DARK_BG, LIGHT_BG] {
            let (r, g, b) = bytes(match_highlight(285.0, bg));
            let (_, out_c, _) = srgb_bytes_to_oklch(r, g, b);
            assert!(
                out_c >= MIN_MATCH_HIGHLIGHT_CHROMA,
                "highlight chroma too low on {bg:?}: {out_c}"
            );
        }
    }

    #[test]
    fn match_highlight_meets_lc_on_both_surfaces() {
        for bg in [DARK_BG, LIGHT_BG] {
            for hue in [25.0, 85.0, 145.0, 205.0, 265.0, 325.0] {
                let (r, g, b) = bytes(match_highlight(hue, bg));
                let lc = apca_contrast((r, g, b), bg).abs();
                assert!(
                    lc >= MATCH_HIGHLIGHT_LC - 0.5,
                    "hue {hue} on {bg:?}: Lc {lc}"
                );
            }
        }
    }
}
