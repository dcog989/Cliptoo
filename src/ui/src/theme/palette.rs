//! The clip-type colour palette.
//!
//! Each type is one OKLCH hue at one of three shared lightness levels, so
//! related types land in the same hue band: media (video → audio → image) in
//! the warm band, documents in the blue band, code and dev/config in the
//! violet band. Members sharing a hue take different levels so same-hue
//! neighbours stay apart. A role may also pin an exact dark-mode colour (rich
//! text is a muted cream).
//!
//! Colours are generated from the table rather than hand-picked, so
//! [`TypeColor::rgb`] can be checked against the APCA floor in the tests below.

use cliptoo_core::color::oklch_to_srgb_bytes;

/// The three OKLCH lightness levels, dark then light. Alternating the level
/// across a family keeps same-hue neighbours distinguishable.
const DARK_LEVELS: [f64; 3] = [0.64, 0.74, 0.84];
const LIGHT_LEVELS: [f64; 3] = [0.38, 0.45, 0.52];

/// OKLCH chroma, per mode. Low-chroma roles scale this down.
const DARK_CHROMA: f64 = 0.11;
const LIGHT_CHROMA: f64 = 0.08;

/// One clip-type colour: OKLCH hue in degrees, a 1-based lightness level, and
/// a chroma scale (1.0 = the mode's full chroma).
#[derive(Clone, Copy)]
pub(super) struct TypeColor {
    hue: f64,
    level: u8,
    chroma_scale: f64,
    /// Exact dark-mode bytes for a role the hue/level model can't express.
    dark: Option<[u8; 3]>,
}

impl TypeColor {
    const fn new(hue: f64, level: u8, chroma_scale: f64) -> Self {
        Self {
            hue,
            level,
            chroma_scale,
            dark: None,
        }
    }

    /// As [`TypeColor::new`], but pins the dark-mode colour (light is still
    /// generated) — for off-model roles like a muted cream.
    const fn with_dark(hue: f64, level: u8, chroma_scale: f64, dark: [u8; 3]) -> Self {
        Self {
            hue,
            level,
            chroma_scale,
            dark: Some(dark),
        }
    }

    /// The sRGB bytes for this role in the given theme mode.
    pub(super) fn rgb(self, is_dark: bool) -> [u8; 3] {
        if is_dark && let Some(dark) = self.dark {
            return dark;
        }
        let (levels, chroma) = if is_dark {
            (DARK_LEVELS, DARK_CHROMA)
        } else {
            (LIGHT_LEVELS, LIGHT_CHROMA)
        };
        oklch_to_srgb_bytes(
            levels[self.level as usize - 1],
            chroma * self.chroma_scale,
            self.hue,
        )
    }
}

// Warm band — danger, then the media trio (video → audio → image).
pub(super) const DANGER: TypeColor = TypeColor::new(25.0, 1, 1.0);
pub(super) const VIDEO: TypeColor = TypeColor::new(55.0, 2, 1.0);
pub(super) const AUDIO: TypeColor = TypeColor::new(85.0, 3, 1.0);
pub(super) const IMAGE: TypeColor = TypeColor::new(115.0, 2, 1.0);
/// Neutral taupe, deliberately desaturated so it reads as "misc bundle"
/// rather than joining the media family.
pub(super) const ARCHIVE: TypeColor = TypeColor::new(80.0, 2, 0.35);

// Teal band — on-disk paths and folders.
pub(super) const PATH: TypeColor = TypeColor::new(175.0, 1, 1.0);
pub(super) const FOLDER: TypeColor = TypeColor::new(175.0, 3, 1.0);

// Blue band — text file → document, then links.
pub(super) const TEXT_FILE: TypeColor = TypeColor::new(210.0, 3, 1.0);
pub(super) const DOCUMENT: TypeColor = TypeColor::new(240.0, 1, 1.0);
pub(super) const LINK: TypeColor = TypeColor::new(265.0, 2, 1.0);

/// Rich text is a muted cream (hue 77) rather than a family hue, so it does
/// not compete with the type colours. The dark value is the reference; light
/// is generated from the same hue at the light band's lightness and chroma.
pub(super) const RICH: TypeColor = TypeColor::with_dark(77.0, 3, 0.54, [0xF2, 0xD7, 0xB0]);

// Violet band — code snippets and dev/config files.
pub(super) const CODE: TypeColor = TypeColor::new(300.0, 2, 1.0);
pub(super) const DEV: TypeColor = TypeColor::new(310.0, 1, 1.0);

// Magenta band — colour clips, then data/tabular files.
pub(super) const COLOR: TypeColor = TypeColor::new(330.0, 2, 1.0);
pub(super) const DATA: TypeColor = TypeColor::new(355.0, 3, 1.0);

// Desaturated slate for "other file" — visible without forming a family.
pub(super) const GENERIC: TypeColor = TypeColor::new(250.0, 1, 0.28);

/// Every role, for the palette contrast test.
#[cfg(test)]
const ALL: [(&str, TypeColor); 16] = [
    ("danger", DANGER),
    ("video", VIDEO),
    ("audio", AUDIO),
    ("image", IMAGE),
    ("archive", ARCHIVE),
    ("path", PATH),
    ("folder", FOLDER),
    ("text_file", TEXT_FILE),
    ("rich", RICH),
    ("document", DOCUMENT),
    ("link", LINK),
    ("code", CODE),
    ("dev", DEV),
    ("color", COLOR),
    ("data", DATA),
    ("generic", GENERIC),
];

#[cfg(test)]
mod tests {
    use super::ALL;
    use crate::theme::contrast::apca_contrast;

    const DARK_BG: (u8, u8, u8) = (0x18, 0x18, 0x18);
    const LIGHT_BG: (u8, u8, u8) = (0xF5, 0xF5, 0xF5);
    /// Floor matching the palette's own weakest role (deep red at Lc ≈ 38 on
    /// the dark row surface).
    const MIN_LC: f64 = 35.0;

    #[test]
    fn every_type_color_clears_the_row_surface() {
        for (name, role) in ALL {
            for (is_dark, bg) in [(true, DARK_BG), (false, LIGHT_BG)] {
                let [r, g, b] = role.rgb(is_dark);
                let lc = apca_contrast((r, g, b), bg).abs();
                assert!(lc >= MIN_LC, "{name} on {bg:?}: Lc {lc:.1} < {MIN_LC}");
            }
        }
    }
}
