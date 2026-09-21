//! Colour-space conversion helpers built on `palette`.
//!
//! Only the conversions the app needs are wrapped: sRGB ↔ OKLCH (theme accent
//! math) and Okhsl → sRGB (the `okhsl()` clip format). The OKLCH → sRGB
//! direction keeps the original gamut mapping — an out-of-gamut colour has its
//! chroma reduced at a fixed lightness/hue rather than being per-channel
//! clamped, so accent colours stay perceptually stable.

use palette::convert::{FromColorUnclamped, IntoColorUnclamped};
use palette::{Okhsl, Oklab, Oklch, Srgb};

/// Maximum plausible OKLCH chroma within the sRGB gamut.
const CHROMA_SEARCH_MAX: f64 = 0.4;
/// Binary-search iterations; precision ≈ CHROMA_SEARCH_MAX / 2^ITERS.
const CHROMA_SEARCH_ITERS: u32 = 10;
/// Gamut tolerance. The transfer function is ~12.9× the linear slope near
/// zero, so this is the gamma-encoded equivalent of a ~1e-4 linear tolerance.
const GAMUT_EPSILON: f64 = 1e-3;

fn to_byte(c: f64) -> u8 {
    (c.clamp(0.0, 1.0) * 255.0).round() as u8
}

fn to_bytes(srgb: Srgb<f64>) -> [u8; 3] {
    let (r, g, b) = srgb.into_components();
    [to_byte(r), to_byte(g), to_byte(b)]
}

fn oklch_to_srgb(l: f64, c: f64, h_deg: f64) -> Srgb<f64> {
    Srgb::from_color_unclamped(Oklch::new(l, c, h_deg))
}

fn is_in_gamut(srgb: Srgb<f64>) -> bool {
    let (r, g, b) = srgb.into_components();
    [r, g, b]
        .iter()
        .all(|&v| (-GAMUT_EPSILON..=1.0 + GAMUT_EPSILON).contains(&v))
}

/// Largest chroma that stays in gamut at a fixed lightness and hue.
fn find_max_chroma(l: f64, h_deg: f64) -> f64 {
    let mut low: f64 = 0.0;
    let mut high: f64 = CHROMA_SEARCH_MAX;
    for _ in 0..CHROMA_SEARCH_ITERS {
        let mid = (low + high) / 2.0;
        if is_in_gamut(oklch_to_srgb(l, mid, h_deg)) {
            low = mid;
        } else {
            high = mid;
        }
    }
    low
}

/// Convert OKLCH (L, C, H degrees) → sRGB bytes, mapping out-of-gamut colours
/// to the nearest in-gamut equivalent by reducing chroma.
pub fn oklch_to_srgb_bytes(l: f64, c: f64, h_deg: f64) -> [u8; 3] {
    let srgb = oklch_to_srgb(l, c, h_deg);
    let srgb = if is_in_gamut(srgb) {
        srgb
    } else {
        oklch_to_srgb(l, find_max_chroma(l, h_deg), h_deg)
    };
    to_bytes(srgb)
}

/// Convert sRGB bytes → OKLCH (L, C, H degrees).
pub fn srgb_bytes_to_oklch(r: u8, g: u8, b: u8) -> (f64, f64, f64) {
    let srgb = Srgb::new(r as f64 / 255.0, g as f64 / 255.0, b as f64 / 255.0);
    let oklab: Oklab<f64> = srgb.into_color_unclamped();
    let chroma = (oklab.a * oklab.a + oklab.b * oklab.b).sqrt();
    let mut hue = oklab.b.atan2(oklab.a).to_degrees();
    if hue < 0.0 {
        hue += 360.0;
    }
    (oklab.l, chroma, hue)
}

/// Convert Okhsl (`h` degrees, `s`/`l` in 0..=1) → sRGB bytes.
pub fn okhsl_to_srgb_bytes(h_deg: f64, s: f64, l: f64) -> [u8; 3] {
    let color = Okhsl::new(h_deg, s.clamp(0.0, 1.0), l.clamp(0.0, 1.0));
    to_bytes(Srgb::from_color_unclamped(color))
}

#[cfg(test)]
mod tests {
    use super::oklch_to_srgb_bytes;

    #[test]
    fn out_of_gamut_chroma_clamps_to_gamut_boundary() {
        // The only custom logic here: chroma beyond the sRGB boundary at a
        // fixed L/H collapses to the same boundary colour.
        let boundary = oklch_to_srgb_bytes(0.452, 0.4, 264.05);
        assert_eq!(oklch_to_srgb_bytes(0.452, 0.5, 264.05), boundary);
        assert_eq!(oklch_to_srgb_bytes(0.452, 0.8, 264.05), boundary);
    }
}
