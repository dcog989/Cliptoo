//! OKLCH → sRGB conversion and gamut mapping.
//!
//! All matrix constants follow the Björn Ottosson OKLab specification naming:
//!   - `M1` / `M1_INV` — OKLab ↔ LMS' mixed matrix
//!   - `M2` / `M2_INV` — LMS ↔ XYZ mixed matrix
//!   - `M_SRGB_TO_XYZ` / `M_XYZ_TO_SRGB` — standard sRGB colour space matrices
//!
//! See PORTING.md §4 for the full algorithm and `FindMaxChroma` gamut mapping.

/// Parsed color result with all representations.
#[derive(Debug, Clone)]
pub struct ParsedColor {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
    pub hex: String,
}

// ── Matrix constants (Björn Ottosson OKLab spec naming) ──────────────────────

/// M1 — OKLab → LMS' mixed matrix.
/// (The inverse, LMS' → OKLab, is M1_INV below.)
#[rustfmt::skip]
const M1: [[f64; 3]; 3] = [
    [1.0,  0.396_337_777_459_8,  0.215_803_757_208_5],
    [1.0, -0.105_561_342_323_8, -0.063_854_173_771_6],
    [1.0, -0.089_484_177_546_8, -1.291_485_548_010_5],
];

/// M2 — LMS → XYZ mixed matrix.
/// (The inverse, XYZ → LMS, is M2_INV below.)
#[rustfmt::skip]
const M2: [[f64; 3]; 3] = [
    [ 1.227_013_851_103_5,  -0.557_799_980_651_8,  0.281_256_148_500_5],
    [-0.040_580_178_423_3,   1.112_256_869_616_4, -0.071_676_691_193_1],
    [-0.076_381_284_505_3,  -0.421_481_978_958_1,  1.586_163_220_634_4],
];

/// M_XYZ_TO_SRGB — XYZ D65 → linear sRGB (standard matrix, independent of OKLab spec).
#[rustfmt::skip]
const M_XYZ_TO_SRGB: [[f64; 3]; 3] = [
    [ 3.240_969_941_904_5, -1.537_383_177_570_1, -0.498_610_760_293_0],
    [-0.969_243_636_280_9,  1.875_967_501_507_1,  0.041_555_057_407_2],
    [ 0.055_630_079_696_8, -0.203_976_958_888_9,  1.056_971_514_242_9],
];

// ── Core conversion ───────────────────────────────────────────────────────────

fn mat3_mul(m: &[[f64; 3]; 3], v: [f64; 3]) -> [f64; 3] {
    [
        m[0][0] * v[0] + m[0][1] * v[1] + m[0][2] * v[2],
        m[1][0] * v[0] + m[1][1] * v[1] + m[1][2] * v[2],
        m[2][0] * v[0] + m[2][1] * v[1] + m[2][2] * v[2],
    ]
}

fn srgb_gamma(c: f64) -> f64 {
    if c <= 0.003_130_8 {
        12.92 * c
    } else {
        1.055 * c.powf(1.0 / 2.4) - 0.055
    }
}

fn to_byte(c: f64) -> u8 {
    (c.clamp(0.0, 1.0) * 255.0).round() as u8
}

/// Convert OKLCH (L, C, H_degrees) → linear sRGB floats (may be out of gamut).
pub fn oklch_to_linear_rgb(l: f64, c: f64, h_deg: f64) -> [f64; 3] {
    let h_rad = h_deg.to_radians();
    let a = c * h_rad.cos();
    let b_coord = c * h_rad.sin();

    // OKLab → LMS'  (M1)
    let lms_prime = mat3_mul(&M1, [l, a, b_coord]);
    // LMS' → LMS (cube)
    let lms = [
        lms_prime[0].powi(3),
        lms_prime[1].powi(3),
        lms_prime[2].powi(3),
    ];
    // LMS → XYZ  (M2)
    let xyz = mat3_mul(&M2, lms);
    // XYZ → linear sRGB
    mat3_mul(&M_XYZ_TO_SRGB, xyz)
}

/// Convert OKLCH → sRGB bytes, clamping to gamut boundary.
/// Uses `find_max_chroma` to map out-of-gamut colors to the nearest in-gamut equivalent.
pub fn oklch_to_srgb_bytes(l: f64, c: f64, h_deg: f64) -> [u8; 3] {
    let rgb_linear = oklch_to_linear_rgb(l, c, h_deg);
    let in_gamut = is_in_srgb_gamut(rgb_linear);

    let c2 = if in_gamut {
        c
    } else {
        find_max_chroma(l, h_deg)
    };

    let rgb_linear = oklch_to_linear_rgb(l, c2, h_deg);
    [
        to_byte(srgb_gamma(rgb_linear[0])),
        to_byte(srgb_gamma(rgb_linear[1])),
        to_byte(srgb_gamma(rgb_linear[2])),
    ]
}

/// Returns true if all linear RGB channels are within [-ε, 1+ε].
fn is_in_srgb_gamut(rgb: [f64; 3]) -> bool {
    const EPS: f64 = 1e-4;
    rgb.iter().all(|&c| (-EPS..=1.0 + EPS).contains(&c))
}

/// Max plausible OKLCH chroma within the sRGB gamut.
const CHROMA_SEARCH_MAX: f64 = 0.4;
/// Binary search iterations — precision ≈ CHROMA_SEARCH_MAX / 2^ITER.
const CHROMA_SEARCH_ITERS: u32 = 10;

/// Binary search for the maximum in-gamut chroma at given L and H.
/// 10 iterations → precision of ~0.4 / 1024 ≈ 0.0004.
fn find_max_chroma(l: f64, h_deg: f64) -> f64 {
    let mut low: f64 = 0.0;
    let mut high: f64 = CHROMA_SEARCH_MAX;
    for _ in 0..CHROMA_SEARCH_ITERS {
        let mid = (low + high) / 2.0;
        let rgb = oklch_to_linear_rgb(l, mid, h_deg);
        if is_in_srgb_gamut(rgb) {
            low = mid;
        } else {
            high = mid;
        }
    }
    low
}

// ── Reverse: sRGB bytes → OKLCH ──────────────────────────────────────────────

/// sRGB gamma expansion (inverse)
fn srgb_linear(c: f64) -> f64 {
    if c <= 0.040_45 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

/// M_SRGB_TO_XYZ — linear sRGB → XYZ D65 (standard).
#[rustfmt::skip]
const M_SRGB_TO_XYZ: [[f64; 3]; 3] = [
    [0.412_390_799_265_9, 0.357_584_339_383_5, 0.180_480_788_401_8],
    [0.212_639_005_871_5, 0.715_168_678_767_0, 0.072_192_315_361_5],
    [0.019_330_818_715_6, 0.119_194_779_794_6, 0.950_532_152_249_7],
];

/// M2_INV — XYZ → LMS (inverse of M2, from the OKLab spec).
/// These are the CSS Color 4 / Björn Ottosson constants; they are the exact
/// inverse of `M2` (older third-party copies of "OKLab matrices" carry a
/// corrupted XYZ → LMS matrix that is NOT `M2⁻¹`).
#[rustfmt::skip]
const M2_INV: [[f64; 3]; 3] = [
    [ 0.819_022_443_216_431_9,  0.361_906_256_052_122_5, -0.128_873_782_616_164],
    [ 0.032_983_667_198_027_1,  0.929_286_846_896_554_6,  0.036_144_668_169_998_4],
    [ 0.048_177_199_566_046_3,  0.264_239_524_944_227_6,  0.633_547_825_813_693_7],
];

/// M1_INV — LMS' → OKLab (inverse of M1, from OKLab spec).
#[rustfmt::skip]
const M1_INV: [[f64; 3]; 3] = [
    [0.210_454_255_340_0, 0.793_617_785_195_0, -0.004_072_046_935_0],
    [1.977_998_495_146_0,-2.428_592_205_032_0,  0.450_593_709_886_0],
    [0.025_904_037_114_0, 0.782_771_766_168_0, -0.808_675_766_036_0],
];

/// Convert sRGB bytes → OKLCH (L, C, H_degrees).
pub fn srgb_bytes_to_oklch(r: u8, g: u8, b: u8) -> (f64, f64, f64) {
    let r_lin = srgb_linear(r as f64 / 255.0);
    let g_lin = srgb_linear(g as f64 / 255.0);
    let b_lin = srgb_linear(b as f64 / 255.0);

    let xyz = mat3_mul(&M_SRGB_TO_XYZ, [r_lin, g_lin, b_lin]);
    let lms = mat3_mul(&M2_INV, xyz);
    let lms_cbrt = [lms[0].cbrt(), lms[1].cbrt(), lms[2].cbrt()];
    let [l, a, b_coord] = mat3_mul(&M1_INV, lms_cbrt);

    let c = (a * a + b_coord * b_coord).sqrt();
    let mut h = b_coord.atan2(a).to_degrees();
    if h < 0.0 {
        h += 360.0;
    }
    (l, c, h)
}

#[cfg(test)]
mod tests {
    use super::{oklch_to_srgb_bytes, srgb_bytes_to_oklch};

    const L_TOL: f64 = 0.002;
    const C_TOL: f64 = 0.002;
    const H_TOL: f64 = 0.3;

    #[test]
    fn known_reference_values_match_css_color_4() {
        // Pure red → OKLCH reference from the CSS Color 4 spec.
        let (l, c, h) = srgb_bytes_to_oklch(255, 0, 0);
        assert!((l - 0.628).abs() < L_TOL, "red L={l}");
        assert!((c - 0.258).abs() < C_TOL, "red C={c}");
        assert!((h - 29.2).abs() < H_TOL, "red H={h}");
        // Pure green.
        let (l, c, h) = srgb_bytes_to_oklch(0, 255, 0);
        assert!((l - 0.866).abs() < L_TOL, "green L={l}");
        assert!((c - 0.295).abs() < C_TOL, "green C={c}");
        assert!((h - 142.5).abs() < H_TOL, "green H={h}");
        // Pure blue.
        let (l, c, h) = srgb_bytes_to_oklch(0, 0, 255);
        assert!((l - 0.452).abs() < L_TOL, "blue L={l}");
        assert!((c - 0.313).abs() < C_TOL, "blue C={c}");
        assert!((h - 264.1).abs() < H_TOL, "blue H={h}");
        // White / black stay neutral.
        let (l, c, _h) = srgb_bytes_to_oklch(255, 255, 255);
        assert!((l - 1.0).abs() < L_TOL && c < C_TOL, "white L={l} C={c}");
        let (l, c, _h) = srgb_bytes_to_oklch(0, 0, 0);
        assert!(l < L_TOL && c < C_TOL, "black L={l} C={c}");
    }

    #[test]
    fn round_trip_stays_near_identity() {
        // Gamut-boundary colors lose a few bytes to the binary-search clip, so
        // allow a small delta; the point is to catch a broken matrix, which
        // produces errors of ~10+ bytes for mid tones.
        for r in (0..=255).step_by(16) {
            for g in (0..=255).step_by(16) {
                for b in (0..=255).step_by(16) {
                    let (l, c, h) = srgb_bytes_to_oklch(r, g, b);
                    let rt = oklch_to_srgb_bytes(l, c, h);
                    assert!(
                        rt[0].abs_diff(r) <= 4 && rt[1].abs_diff(g) <= 4 && rt[2].abs_diff(b) <= 4,
                        "round trip drifted for rgb({r},{g},{b}) -> {rt:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn forward_conversion_known_values() {
        // Neutral colors: L alone drives the output, chroma 0 is achromatic.
        assert_eq!(oklch_to_srgb_bytes(1.0, 0.0, 0.0), [255, 255, 255]);
        assert_eq!(oklch_to_srgb_bytes(0.0, 0.0, 0.0), [0, 0, 0]);
        assert_eq!(oklch_to_srgb_bytes(0.5, 0.0, 0.0), [99, 99, 99]);
        // Pure red reconstructed from its CSS Color 4 OKLCH reference values.
        let [r, g, b] = oklch_to_srgb_bytes(0.628, 0.2577, 29.23);
        assert!(r == 255 && g <= 1 && b <= 1, "red-ish got [{r},{g},{b}]");
    }

    #[test]
    fn out_of_gamut_chroma_clamps_to_gamut_boundary() {
        // Any chroma beyond the sRGB boundary at a fixed L/H collapses to the
        // same gamut-boundary color (FindMaxChroma).
        let boundary = oklch_to_srgb_bytes(0.452, 0.4, 264.05);
        assert_eq!(oklch_to_srgb_bytes(0.452, 0.5, 264.05), boundary);
        assert_eq!(oklch_to_srgb_bytes(0.452, 0.6, 264.05), boundary);
        assert_eq!(oklch_to_srgb_bytes(0.452, 0.8, 264.05), boundary);
        // High-chroma input at any hue stays clamped to that hue's boundary.
        for h in (0..360).step_by(15) {
            let rgb = oklch_to_srgb_bytes(0.5, 0.5, h as f64);
            assert_eq!(rgb, oklch_to_srgb_bytes(0.5, 0.8, h as f64), "hue {h}");
        }
    }
}
