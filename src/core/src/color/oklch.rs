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
pub fn find_max_chroma(l: f64, h_deg: f64) -> f64 {
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

/// M2_INV — XYZ → LMS (inverse of M2, from OKLab spec).
#[rustfmt::skip]
const M2_INV: [[f64; 3]; 3] = [
    [ 0.818_933_501_615_9,  0.328_845_301_486_4, -0.148_537_138_960_8],
    [-0.032_592_039_985_3,  0.936_932_803_485_9,  0.031_152_878_378_8],
    [ 0.048_177_199_566_3,  0.174_288_981_061_0,  0.692_397_143_327_5],
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

/// Chroma level multipliers (proportion of FindMaxChroma result).
pub fn chroma_level_factor(level: &str) -> f64 {
    match level {
        "Neon" => 1.0,
        "Vibrant" => 0.70,
        "Mellow" => 0.50,
        "Muted" => 0.35,
        "Ditchwater" => 0.20,
        _ => 0.50,
    }
}
