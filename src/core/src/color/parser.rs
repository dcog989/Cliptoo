use super::oklch::{ParsedColor, oklch_to_srgb_bytes};

macro_rules! named_color {
    ($($name:ident => ($r:expr, $g:expr, $b:expr)),* $(,)?) => {
        fn named_color(s: &str) -> Option<ParsedColor> {
            let c = match s {
                $(stringify!($name) => ($r, $g, $b),)*
                _ => return None,
            };
            Some(ParsedColor {
                r: c.0, g: c.1, b: c.2, a: 255,
                hex: format!("#{:02X}{:02X}{:02X}", c.0, c.1, c.2),
            })
        }
    };
}

named_color! {
    aliceblue => (240, 248, 255), antiquewhite => (250, 235, 215),
    aqua => (0, 255, 255), aquamarine => (127, 255, 212),
    azure => (240, 255, 255), beige => (245, 245, 220),
    bisque => (255, 228, 196), black => (0, 0, 0),
    blanchedalmond => (255, 235, 205), blue => (0, 0, 255),
    blueviolet => (138, 43, 226), brown => (165, 42, 42),
    burlywood => (222, 184, 135), cadetblue => (95, 158, 160),
    chartreuse => (127, 255, 0), chocolate => (210, 105, 30),
    coral => (255, 127, 80), cornflowerblue => (100, 149, 237),
    cornsilk => (255, 248, 220), crimson => (220, 20, 60),
    cyan => (0, 255, 255), darkblue => (0, 0, 139),
    darkcyan => (0, 139, 139), darkgoldenrod => (184, 134, 11),
    darkgray => (169, 169, 169), darkgreen => (0, 100, 0),
    darkgrey => (169, 169, 169), darkkhaki => (189, 183, 107),
    darkmagenta => (139, 0, 139), darkolivegreen => (85, 107, 47),
    darkorange => (255, 140, 0), darkorchid => (153, 50, 204),
    darkred => (139, 0, 0), darksalmon => (233, 150, 122),
    darkseagreen => (143, 188, 143), darkslateblue => (72, 61, 139),
    darkslategray => (47, 79, 79), darkslategrey => (47, 79, 79),
    darkturquoise => (0, 206, 209), darkviolet => (148, 0, 211),
    deeppink => (255, 20, 147), deepskyblue => (0, 191, 255),
    dimgray => (105, 105, 105), dimgrey => (105, 105, 105),
    dodgerblue => (30, 144, 255), firebrick => (178, 34, 34),
    floralwhite => (255, 250, 240), forestgreen => (34, 139, 34),
    fuchsia => (255, 0, 255), gainsboro => (220, 220, 220),
    ghostwhite => (248, 248, 255), gold => (255, 215, 0),
    goldenrod => (218, 165, 32), gray => (128, 128, 128),
    green => (0, 128, 0), greenyellow => (173, 255, 47),
    grey => (128, 128, 128), honeydew => (240, 255, 240),
    hotpink => (255, 105, 180), indianred => (205, 92, 92),
    indigo => (75, 0, 130), ivory => (255, 255, 240),
    khaki => (240, 230, 140), lavender => (230, 230, 250),
    lavenderblush => (255, 240, 245), lawngreen => (124, 252, 0),
    lemonchiffon => (255, 250, 205), lightblue => (173, 216, 230),
    lightcoral => (240, 128, 128), lightcyan => (224, 255, 255),
    lightgoldenrodyellow => (250, 250, 210), lightgray => (211, 211, 211),
    lightgreen => (144, 238, 144), lightgrey => (211, 211, 211),
    lightpink => (255, 182, 193), lightsalmon => (255, 160, 122),
    lightseagreen => (32, 178, 170), lightskyblue => (135, 206, 250),
    lightslategray => (119, 136, 153), lightslategrey => (119, 136, 153),
    lightsteelblue => (176, 196, 222), lightyellow => (255, 255, 224),
    lime => (0, 255, 0), limegreen => (50, 205, 50),
    linen => (250, 240, 230), magenta => (255, 0, 255),
    maroon => (128, 0, 0), mediumaquamarine => (102, 205, 170),
    mediumblue => (0, 0, 205), mediumorchid => (186, 85, 211),
    mediumpurple => (147, 112, 219), mediumseagreen => (60, 179, 113),
    mediumslateblue => (123, 104, 238), mediumspringgreen => (0, 250, 154),
    mediumturquoise => (72, 209, 204), mediumvioletred => (199, 21, 133),
    midnightblue => (25, 25, 112), mintcream => (245, 255, 250),
    mistyrose => (255, 228, 225), moccasin => (255, 228, 181),
    navajowhite => (255, 222, 173), navy => (0, 0, 128),
    oldlace => (253, 245, 230), olive => (128, 128, 0),
    olivedrab => (107, 142, 35), orange => (255, 165, 0),
    orangered => (255, 69, 0), orchid => (218, 112, 214),
    palegoldenrod => (238, 232, 170), palegreen => (152, 251, 152),
    paleturquoise => (175, 238, 238), palevioletred => (219, 112, 147),
    papayawhip => (255, 239, 213), peachpuff => (255, 218, 185),
    peru => (205, 133, 63), pink => (255, 192, 203),
    plum => (221, 160, 221), powderblue => (176, 224, 230),
    purple => (128, 0, 128), rebeccapurple => (102, 51, 153),
    red => (255, 0, 0), rosybrown => (188, 143, 143),
    royalblue => (65, 105, 225), saddlebrown => (139, 69, 19),
    salmon => (250, 128, 114), sandybrown => (244, 164, 96),
    seagreen => (46, 139, 87), seashell => (255, 245, 238),
    sienna => (160, 82, 45), silver => (192, 192, 192),
    skyblue => (135, 206, 235), slateblue => (106, 90, 205),
    slategray => (112, 128, 144), slategrey => (112, 128, 144),
    snow => (255, 250, 250), springgreen => (0, 255, 127),
    steelblue => (70, 130, 180), tan => (210, 180, 140),
    teal => (0, 128, 128), thistle => (216, 191, 216),
    tomato => (255, 99, 71), turquoise => (64, 224, 208),
    violet => (238, 130, 238), wheat => (245, 222, 179),
    white => (255, 255, 255), whitesmoke => (245, 245, 245),
    yellow => (255, 255, 0), yellowgreen => (154, 205, 50),
}

/// Parse a single hex channel given as 1 nibble (short) or 2 nibbles (full).
/// Short form: expand `N` → `NN` via `N * 17` (0x11 = 17).
/// Full form:  parse the 2-char byte directly.
#[inline]
fn hex_channel(s: &str) -> Option<u8> {
    match s.len() {
        1 => u8::from_str_radix(s, 16).ok().map(|n| n * 17),
        2 => u8::from_str_radix(s, 16).ok(),
        _ => None,
    }
}

fn parse_hex(s: &str) -> Option<ParsedColor> {
    let s = s.strip_prefix('#').unwrap_or(s);
    // w is the per-channel width in nibbles: 1 for short (#RGB/#RGBA), 2 for full (#RRGGBB/#RRGGBBAA).
    let (w, has_alpha) = match s.len() {
        3 => (1, false),
        4 => (1, true),
        6 => (2, false),
        8 => (2, true),
        _ => return None,
    };
    let r = hex_channel(&s[0..w])?;
    let g = hex_channel(&s[w..2 * w])?;
    let b = hex_channel(&s[2 * w..3 * w])?;
    let a = if has_alpha {
        hex_channel(&s[3 * w..4 * w])?
    } else {
        255
    };
    Some(ParsedColor {
        r,
        g,
        b,
        a,
        hex: format!("#{r:02X}{g:02X}{b:02X}"),
    })
}

fn parse_0x(s: &str) -> Option<ParsedColor> {
    // Assumes Android/Java ARGB byte order: 0xAARRGGBB.
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

fn strip_func(s: &str) -> Option<(&str, &str)> {
    let paren = s.find('(')?;
    let name = s[..paren].trim();
    let body = s[paren + 1..].strip_suffix(')')?.trim();
    Some((name, body))
}

fn parse_percent_or_f64(s: &str) -> Option<f64> {
    let s = s.trim();
    if let Some(pct) = s.strip_suffix('%') {
        pct.trim().parse::<f64>().ok().map(|v| v / 100.0)
    } else {
        s.parse::<f64>().ok()
    }
}

fn parse_angle(s: &str) -> Option<f64> {
    let s = s.trim();
    if let Some(v) = s.strip_suffix("deg") {
        v.trim().parse::<f64>().ok()
    } else if let Some(v) = s.strip_suffix("rad") {
        v.trim().parse::<f64>().ok().map(|r| r.to_degrees())
    } else if let Some(v) = s.strip_suffix("grad") {
        v.trim().parse::<f64>().ok().map(|g| g * 0.9)
    } else if let Some(v) = s.strip_suffix("turn") {
        v.trim().parse::<f64>().ok().map(|t| t * 360.0)
    } else {
        s.parse::<f64>().ok()
    }
}

fn split_args(body: &str) -> Vec<&str> {
    if body.contains(',') {
        body.split(',').map(|s| s.trim()).collect()
    } else if let Some(slash) = body.find('/') {
        let mut parts: Vec<&str> = body[..slash].split_whitespace().map(|s| s.trim()).collect();
        parts.push(body[slash + 1..].trim());
        parts
    } else {
        body.split_whitespace().map(|s| s.trim()).collect()
    }
}

fn parse_rgb_body(body: &str) -> Option<ParsedColor> {
    let args = split_args(body);
    if args.len() < 3 || args.len() > 4 {
        return None;
    }
    // CSS spec: if any channel uses %, all are treated as 0–100% → 0.0–1.0.
    // If none use %, they are 0–255 integers (the common `rgb(255, 0, 128)` form).
    let any_percent = args[..3].iter().any(|a| a.trim_end().ends_with('%'));
    let to_unit: fn(f64) -> f64 = if any_percent {
        |v| v // parse_percent_or_f64 already divides by 100
    } else {
        |v| v / 255.0 // bare number: 0–255 → 0.0–1.0
    };
    let r = to_unit(parse_percent_or_f64(args[0])?);
    let g = to_unit(parse_percent_or_f64(args[1])?);
    let b = to_unit(parse_percent_or_f64(args[2])?);
    let a = args
        .get(3)
        .and_then(|v| parse_percent_or_f64(v))
        .unwrap_or(1.0);
    Some(ParsedColor {
        r: (r.clamp(0.0, 1.0) * 255.0).round() as u8,
        g: (g.clamp(0.0, 1.0) * 255.0).round() as u8,
        b: (b.clamp(0.0, 1.0) * 255.0).round() as u8,
        a: (a.clamp(0.0, 1.0) * 255.0).round() as u8,
        hex: format!(
            "#{:02X}{:02X}{:02X}",
            (r.clamp(0.0, 1.0) * 255.0).round() as u8,
            (g.clamp(0.0, 1.0) * 255.0).round() as u8,
            (b.clamp(0.0, 1.0) * 255.0).round() as u8
        ),
    })
}

fn parse_hsl_body(body: &str) -> Option<ParsedColor> {
    let args = split_args(body);
    if args.len() < 3 || args.len() > 4 {
        return None;
    }
    let h = parse_angle(args[0])?;
    let s = parse_percent_or_f64(args[1])?;
    let l = parse_percent_or_f64(args[2])?;
    let a = args
        .get(3)
        .and_then(|v| parse_percent_or_f64(v))
        .unwrap_or(1.0);
    hsl_to_rgb(h, s, l).map(|(r, g, b)| ParsedColor {
        r,
        g,
        b,
        a: (a.clamp(0.0, 1.0) * 255.0).round() as u8,
        hex: format!("#{r:02X}{g:02X}{b:02X}"),
    })
}

fn hsl_to_rgb(h: f64, s: f64, l: f64) -> Option<(u8, u8, u8)> {
    let h = ((h % 360.0) + 360.0) % 360.0;
    let s = s.clamp(0.0, 1.0);
    let l = l.clamp(0.0, 1.0);
    let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
    let x = c * (1.0 - ((h / 60.0) % 2.0 - 1.0).abs());
    let m = l - c / 2.0;
    let (r1, g1, b1) = match h as u32 {
        0..=59 => (c, x, 0.0),
        60..=119 => (x, c, 0.0),
        120..=179 => (0.0, c, x),
        180..=239 => (0.0, x, c),
        240..=299 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    Some((
        ((r1 + m) * 255.0).round() as u8,
        ((g1 + m) * 255.0).round() as u8,
        ((b1 + m) * 255.0).round() as u8,
    ))
}

fn parse_hsv_body(body: &str) -> Option<ParsedColor> {
    let args = split_args(body);
    if args.len() < 3 || args.len() > 4 {
        return None;
    }
    let h = parse_angle(args[0])?;
    let s = parse_percent_or_f64(args[1])?;
    let v = parse_percent_or_f64(args[2])?;
    let a = args
        .get(3)
        .and_then(|v| parse_percent_or_f64(v))
        .unwrap_or(1.0);
    hsv_to_rgb(h, s, v).map(|(r, g, b)| ParsedColor {
        r,
        g,
        b,
        a: (a.clamp(0.0, 1.0) * 255.0).round() as u8,
        hex: format!("#{r:02X}{g:02X}{b:02X}"),
    })
}

fn hsv_to_rgb(h: f64, s: f64, v: f64) -> Option<(u8, u8, u8)> {
    let h = ((h % 360.0) + 360.0) % 360.0;
    let s = s.clamp(0.0, 1.0);
    let v = v.clamp(0.0, 1.0);
    let c = v * s;
    let x = c * (1.0 - ((h / 60.0) % 2.0 - 1.0).abs());
    let m = v - c;
    let (r1, g1, b1) = match h as u32 {
        0..=59 => (c, x, 0.0),
        60..=119 => (x, c, 0.0),
        120..=179 => (0.0, c, x),
        180..=239 => (0.0, x, c),
        240..=299 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    Some((
        ((r1 + m) * 255.0).round() as u8,
        ((g1 + m) * 255.0).round() as u8,
        ((b1 + m) * 255.0).round() as u8,
    ))
}

fn parse_cmyk_body(body: &str) -> Option<ParsedColor> {
    let args = split_args(body);
    if args.len() != 4 {
        return None;
    }
    let c = parse_percent_or_f64(args[0])?;
    let m = parse_percent_or_f64(args[1])?;
    let y = parse_percent_or_f64(args[2])?;
    let k = parse_percent_or_f64(args[3])?;
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

fn parse_oklch_body(body: &str) -> Option<ParsedColor> {
    let args = split_args(body);
    if args.len() < 3 || args.len() > 4 {
        return None;
    }
    let l = parse_percent_or_f64(args[0])?;
    let c = parse_percent_or_f64(args[1])?;
    let h = parse_angle(args[2])?;
    let [r, g, b] = oklch_to_srgb_bytes(l, c, h);
    Some(ParsedColor {
        r,
        g,
        b,
        a: 255,
        hex: format!("#{r:02X}{g:02X}{b:02X}"),
    })
}

fn parse_function(s: &str) -> Option<ParsedColor> {
    let (name, body) = strip_func(s)?;
    match name.to_ascii_lowercase().as_str() {
        "rgb" | "rgba" => parse_rgb_body(body),
        "hsl" | "hsla" => parse_hsl_body(body),
        "hsv" | "hsva" => parse_hsv_body(body),
        "cmyk" => parse_cmyk_body(body),
        "oklch" | "oklch()" => parse_oklch_body(body),
        _ => None,
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
        if s.starts_with('#') {
            return parse_hex(s);
        }
        if s.starts_with("0x") || s.starts_with("0X") {
            return parse_0x(s);
        }
        if let Some(paren) = s.find('(')
            && paren > 0
        {
            return parse_function(s);
        }
        named_color(s)
    }
}
