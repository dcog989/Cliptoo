//! CSS color string parsing backed by [`csscolorparser`].
//!
//! `csscolorparser` implements CSS Color Module Level 4: named colors, hex,
//! `rgb()`, `hsl()`, `hwb()`, `lab()`, `lch()`, `oklab()`, `oklch()`, plus
//! the non-standard `hsv()`. Two formats remain here because the crate does
//! not understand them: Android/Java `0xAARRGGBB` integers and `cmyk()`.

/// Parsed color result with all representations.
#[derive(Debug, Clone)]
pub struct ParsedColor {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
    pub hex: String,
}

/// Android/Java `0xAARRGGBB` integer literal → parsed color (ARGB byte order).
fn parse_argb_0x(s: &str) -> Option<ParsedColor> {
    let s = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X"))?;
    if s.len() != 8 {
        return None;
    }
    let a = u8::from_str_radix(&s[0..2], 16).ok()?;
    let r = u8::from_str_radix(&s[2..4], 16).ok()?;
    let g = u8::from_str_radix(&s[4..6], 16).ok()?;
    let b = u8::from_str_radix(&s[6..8], 16).ok()?;
    Some(ParsedColor {
        r,
        g,
        b,
        a,
        hex: format!("#{r:02X}{g:02X}{b:02X}"),
    })
}

/// `cmyk()` is not part of CSS Color 4, so `csscolorparser` rejects it.
fn parse_cmyk(s: &str) -> Option<ParsedColor> {
    let paren = s.find('(')?;
    if !s[..paren].trim().eq_ignore_ascii_case("cmyk") {
        return None;
    }
    let body = s[paren + 1..].strip_suffix(')')?.trim();
    let args: Vec<&str> = if body.contains(',') {
        body.split(',').map(|a| a.trim()).collect()
    } else {
        body.split_whitespace().map(|a| a.trim()).collect()
    };
    if args.len() != 4 {
        return None;
    }
    let channel = |v: &str| -> Option<f64> {
        if let Some(p) = v.strip_suffix('%') {
            p.trim().parse::<f64>().ok().map(|x| x / 100.0)
        } else {
            v.parse::<f64>().ok()
        }
    };
    let c = channel(args[0])?;
    let m = channel(args[1])?;
    let y = channel(args[2])?;
    let k = channel(args[3])?;
    let r = (255.0 * (1.0 - c.clamp(0.0, 1.0)) * (1.0 - k.clamp(0.0, 1.0))).round() as u8;
    let g = (255.0 * (1.0 - m.clamp(0.0, 1.0)) * (1.0 - k.clamp(0.0, 1.0))).round() as u8;
    let b = (255.0 * (1.0 - y.clamp(0.0, 1.0)) * (1.0 - k.clamp(0.0, 1.0))).round() as u8;
    Some(ParsedColor {
        r,
        g,
        b,
        a: 255,
        hex: format!("#{r:02X}{g:02X}{b:02X}"),
    })
}

/// A 3/4/6/8-character string of only hex digits without a `#` prefix.
fn is_bare_hex(s: &str) -> bool {
    matches!(s.len(), 3 | 4 | 6 | 8) && s.bytes().all(|b| b.is_ascii_hexdigit())
}

fn to_parsed(c: csscolorparser::Color) -> ParsedColor {
    let [r, g, b, a] = c.to_rgba8();
    ParsedColor {
        r,
        g,
        b,
        a,
        hex: format!("#{r:02X}{g:02X}{b:02X}"),
    }
}

pub struct ColorParser;

impl ColorParser {
    pub fn is_color(s: &str) -> bool {
        Self::try_parse(s).is_some()
    }

    pub fn try_parse(s: &str) -> Option<ParsedColor> {
        let s = s.trim();
        if s.is_empty() {
            return None;
        }
        if s.starts_with("0x") || s.starts_with("0X") {
            return parse_argb_0x(s);
        }
        if s.starts_with('#') {
            return Some(to_parsed(csscolorparser::parse(s).ok()?));
        }
        // csscolorparser accepts bare hex like `ff0000`; the previous parser
        // did not. Reject hex-shaped words ("beef", "123456") so arbitrary
        // text is not misclassified as a color.
        if is_bare_hex(s) {
            return None;
        }
        if let Some(c) = parse_cmyk(s) {
            return Some(c);
        }
        Some(to_parsed(csscolorparser::parse(s).ok()?))
    }
}

#[cfg(test)]
mod tests {
    use super::ColorParser;

    #[test]
    fn hwb_parses_commas_and_whitespace() {
        assert_eq!(
            ColorParser::try_parse("hwb(0, 0%, 0%)").unwrap().hex,
            "#FF0000"
        );
        assert_eq!(
            ColorParser::try_parse("hwb(120 0% 0%)").unwrap().hex,
            "#00FF00"
        );
    }

    #[test]
    fn hwb_blends_toward_white() {
        assert_eq!(
            ColorParser::try_parse("hwb(0 50% 0%)").unwrap().hex,
            "#FF8080"
        );
    }

    #[test]
    fn hwb_overflow_is_gray() {
        assert_eq!(
            ColorParser::try_parse("hwb(0 50% 50%)").unwrap().hex,
            "#808080"
        );
        assert_eq!(
            ColorParser::try_parse("hwb(0 20% 80%)").unwrap().hex,
            "#333333"
        );
    }

    #[test]
    fn hwb_handles_alpha() {
        assert_eq!(ColorParser::try_parse("hwb(0 0% 0% / 50%)").unwrap().a, 128);
        assert_eq!(
            ColorParser::try_parse("hwba(0, 0%, 0%, 0.25)").unwrap().a,
            64
        );
    }

    #[test]
    fn hex_forms() {
        assert_eq!(ColorParser::try_parse("#ff0000").unwrap().hex, "#FF0000");
        assert_eq!(ColorParser::try_parse("#ABC").unwrap().hex, "#AABBCC");
        assert_eq!(ColorParser::try_parse("#F00").unwrap().hex, "#FF0000");
        assert_eq!(ColorParser::try_parse("#ff00007f").unwrap().a, 127);
        assert_eq!(ColorParser::try_parse("#ABCD").unwrap().a, 0xDD);
        assert!(ColorParser::try_parse("#f00").is_some());
        assert!(ColorParser::try_parse("#f0000").is_none());
    }

    #[test]
    fn named_colors_are_case_insensitive() {
        for s in [
            "red",
            "RED",
            "Red",
            "rebeccapurple",
            "cornflowerblue",
            "transparent",
        ] {
            assert!(ColorParser::is_color(s), "for {s}");
        }
        assert_eq!(ColorParser::try_parse("red").unwrap().hex, "#FF0000");
        assert_eq!(ColorParser::try_parse("transparent").unwrap().a, 0);
    }

    #[test]
    fn rgb_hsl_forms() {
        assert_eq!(
            ColorParser::try_parse("rgb(255, 0, 128)").unwrap().hex,
            "#FF0080"
        );
        assert_eq!(
            ColorParser::try_parse("rgb(100%, 0%, 0%)").unwrap().hex,
            "#FF0000"
        );
        assert_eq!(
            ColorParser::try_parse("hsl(120, 100%, 50%)").unwrap().hex,
            "#00FF00"
        );
        assert_eq!(
            ColorParser::try_parse("HSV(0 100% 100%)").unwrap().hex,
            "#FF0000"
        );
    }

    #[test]
    fn modern_color_spaces() {
        for s in [
            "lab(100% 0 0)",
            "lab(0% 0 0)",
            "lch(50% 40 130)",
            "oklab(1 0 0)",
            "oklab(50% 0.1 0.1)",
            "oklch(50% 0.2 240)",
        ] {
            assert!(ColorParser::is_color(s), "for {s}");
        }
        assert_eq!(
            ColorParser::try_parse("lab(100% 0 0)").unwrap().hex,
            "#FFFFFF"
        );
        assert_eq!(
            ColorParser::try_parse("lab(0% 0 0)").unwrap().hex,
            "#000000"
        );
        assert_eq!(
            ColorParser::try_parse("oklab(1 0 0)").unwrap().hex,
            "#FFFFFF"
        );
        assert_eq!(
            ColorParser::try_parse("oklab(0 0 0)").unwrap().hex,
            "#000000"
        );
        assert_eq!(
            ColorParser::try_parse("oklch(100% 0 0)").unwrap().hex,
            "#FFFFFF"
        );
        assert_eq!(
            ColorParser::try_parse("oklch(0% 0 0)").unwrap().hex,
            "#000000"
        );
    }

    #[test]
    fn argb_0x_integer_literals() {
        assert_eq!(ColorParser::try_parse("0xFF880044").unwrap().hex, "#880044");
        assert_eq!(ColorParser::try_parse("0xFF880044").unwrap().a, 255);
        assert_eq!(ColorParser::try_parse("0x7F80FF00").unwrap().a, 127);
        assert!(ColorParser::try_parse("0xzz").is_none());
    }

    #[test]
    fn cmyk() {
        assert_eq!(
            ColorParser::try_parse("cmyk(100%, 0%, 0%, 0%)")
                .unwrap()
                .hex,
            "#00FFFF"
        );
        assert_eq!(
            ColorParser::try_parse("cmyk(0 0 0 0)").unwrap().hex,
            "#FFFFFF"
        );
        assert_eq!(
            ColorParser::try_parse("cmyk(0, 0, 0, 100%)").unwrap().hex,
            "#000000"
        );
        assert_eq!(
            ColorParser::try_parse("CMYK(0 0 0 50%)").unwrap().hex,
            "#808080"
        );
    }

    #[test]
    fn bare_hex_shaped_text_is_not_a_color() {
        for s in ["ff0000", "deadbeef", "beef", "123456", "abc", "f00"] {
            assert!(!ColorParser::is_color(s), "for {s}");
        }
    }

    #[test]
    fn classifies_as_color() {
        assert!(ColorParser::is_color("hwb(240 20% 10%)"));
        assert!(ColorParser::is_color("#ff0000"));
        assert!(!ColorParser::is_color("not a color"));
    }
}
