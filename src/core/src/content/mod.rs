// Content classification pipeline (Phase 1)
// Order: trim → empty → rtf → url → color → filepath → code_heuristic → text

pub mod classifier;
pub mod filetype;
pub mod hash;
pub mod html;
pub mod preview;
pub mod rtf;

pub use classifier::ContentProcessor;
pub use hash::{normalize_line_endings, sha256_hex, sha256_hex_and_prefix, sha256_u64};
pub use html::strip_html;
pub use rtf::strip_rtf;

/// True when `s` is SVG markup — an `<svg>` root element, optionally preceded
/// by an XML prolog or comments — as opposed to an HTML document.
///
/// SVG shares HTML's `<tag>…</tag>` shape, so the classifier must recognise it
/// before `is_html`: `strip_html` reduces an SVG source to empty plain text,
/// which makes both the stored preview and any paste blank.
pub fn is_svg_markup(s: &str) -> bool {
    let mut body = s.strip_prefix('\u{feff}').unwrap_or(s).trim_start();
    loop {
        if let Some(rest) = body.strip_prefix("<?xml") {
            match rest.find("?>") {
                Some(end) => body = rest[end + 2..].trim_start(),
                None => return false,
            }
        } else if let Some(rest) = body.strip_prefix("<!--") {
            match rest.find("-->") {
                Some(end) => body = rest[end + 3..].trim_start(),
                None => return false,
            }
        } else if body.starts_with("<!") {
            // DOCTYPE or another declaration: skip to its closing `>`.
            match body.find('>') {
                Some(end) => body = body[end + 1..].trim_start(),
                None => return false,
            }
        } else {
            break;
        }
    }
    starts_with_svg_tag(body)
}

/// Whether `body` starts with an `<svg…>` opening tag. Case-insensitive and
/// tolerant of a namespace prefix (`<svg:svg`).
fn starts_with_svg_tag(body: &str) -> bool {
    let Some(rest) = body.strip_prefix('<') else {
        return false;
    };
    if !rest
        .get(..3)
        .is_some_and(|name| name.eq_ignore_ascii_case("svg"))
    {
        return false;
    }
    match rest.as_bytes().get(3) {
        None => true,
        Some(&c) => c.is_ascii_whitespace() || matches!(c, b'>' | b'/' | b':'),
    }
}

/// Decode percent-encoded bytes in a `file://` URI path back to a filesystem path.
///
/// Only `%` sequences followed by two hex digits are decoded. Malformed
/// sequences (a lone `%`, a `%` followed by non-hex, or a trailing `%`) are
/// left as-is — never decoded into a corrupted byte.
pub fn percent_decode_path(s: &str) -> String {
    let sb = s.as_bytes();
    let mut out = Vec::with_capacity(s.len());
    let mut i = 0;
    while i < sb.len() {
        let triple = match (hex_val(sb.get(i + 1)), hex_val(sb.get(i + 2))) {
            (Some(hi), Some(lo)) if sb[i] == b'%' => Some((hi * 16 + lo, 3)),
            _ => None,
        };
        match triple {
            Some((b, n)) => {
                out.push(b);
                i += n;
            }
            None => {
                out.push(sb[i]);
                i += 1;
            }
        }
    }
    String::from_utf8(out).unwrap_or_else(|_| s.to_string())
}

/// Parse a single hex digit; `None` for anything else or end of input.
#[inline]
pub(crate) fn hex_val(b: Option<&u8>) -> Option<u8> {
    b.and_then(|x| match *x {
        b'0'..=b'9' => Some(*x - b'0'),
        b'a'..=b'f' => Some(*x - b'a' + 10),
        b'A'..=b'F' => Some(*x - b'A' + 10),
        _ => None,
    })
}

#[cfg(test)]
mod tests {
    use super::{is_svg_markup, percent_decode_path};

    #[test]
    fn decodes_valid_sequences() {
        assert_eq!(percent_decode_path("/home/foo%20bar"), "/home/foo bar");
        assert_eq!(percent_decode_path("/tmp/%41%42"), "/tmp/AB");
        assert_eq!(percent_decode_path("file%2Fpath"), "file/path");
    }

    #[test]
    fn leaves_malformed_sequences_untouched() {
        assert_eq!(percent_decode_path("/home/100%"), "/home/100%");
        assert_eq!(percent_decode_path("/tmp/%zz"), "/tmp/%zz");
        assert_eq!(percent_decode_path("/tmp/%2"), "/tmp/%2");
        assert_eq!(percent_decode_path("50%discount"), "50%discount");
    }

    #[test]
    fn empty_and_plain_inputs_pass_through() {
        assert_eq!(percent_decode_path(""), "");
        assert_eq!(percent_decode_path("/plain/path"), "/plain/path");
    }

    #[test]
    fn detects_svg_markup() {
        for s in [
            "<svg xmlns=\"http://www.w3.org/2000/svg\"><circle/></svg>",
            "  <svg/>",
            "<?xml version=\"1.0\"?><svg viewBox=\"0 0 24 24\"></svg>",
            "<?xml version=\"1.0\"?><!-- c --><svg:svg xmlns:svg=\"x\"></svg:svg>",
            "<!DOCTYPE svg PUBLIC \"-//W3C//DTD SVG 1.1//EN\" \"x.dtd\"><svg></svg>",
            "\u{feff}<SVG></SVG>",
            "<svg",
        ] {
            assert!(is_svg_markup(s), "for {s:?}");
        }
    }

    #[test]
    fn rejects_non_svg_markup() {
        for s in [
            "",
            "<html><body>hi</body></html>",
            "<div>x</div>",
            "<svgfoo></svgfoo>",
            "2 < 3",
        ] {
            assert!(!is_svg_markup(s), "for {s:?}");
        }
    }
}
