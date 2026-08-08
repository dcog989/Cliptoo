// Content classification pipeline (Phase 1)
// Order: trim → empty → rtf → url → color → filepath → code_heuristic → text

pub mod classifier;
pub mod filetype;
pub mod hash;
pub mod preview;
pub mod rtf;

pub use classifier::ContentProcessor;
pub use hash::{normalize_line_endings, sha256_hex, sha256_hex_and_prefix, sha256_u64};
pub use rtf::strip_rtf;

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

/// Percent-encode a filesystem path into a `file:` URI path component.
///
/// Inverse of `percent_decode_path`. Keeps unreserved characters and `/`
/// (path separators) intact and percent-encodes every other byte, so the round
/// trip through `percent_decode_path` is lossless — spaces, colons, non-ASCII
/// text, and literal `%` all survive.
pub fn percent_encode_path(s: &str) -> String {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    let mut out = String::with_capacity(s.len());
    for b in s.as_bytes() {
        match *b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' | b'/' => {
                out.push(*b as char);
            }
            _ => {
                out.push('%');
                out.push(HEX[(b >> 4) as usize] as char);
                out.push(HEX[(b & 0x0F) as usize] as char);
            }
        }
    }
    out
}

/// Parse a single hex digit; `None` for anything else or end of input.
#[inline]
fn hex_val(b: Option<&u8>) -> Option<u8> {
    b.and_then(|x| match *x {
        b'0'..=b'9' => Some(*x - b'0'),
        b'a'..=b'f' => Some(*x - b'a' + 10),
        b'A'..=b'F' => Some(*x - b'A' + 10),
        _ => None,
    })
}

#[cfg(test)]
mod tests {
    use super::percent_decode_path;

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
    fn percent_encode_round_trips() {
        use super::percent_encode_path;
        for path in [
            "/home/foo bar",
            "/home/foo%20bar",
            "/tmp/Ünïcödé/名前.txt",
            "/tmp/file:name",
            "/home/a/b/c.txt",
            "relative/path",
            "/home/dot.dot-dash_dash_tilde~",
        ] {
            let encoded = percent_encode_path(path);
            assert_eq!(percent_decode_path(&encoded), path, "for {path}");
        }
    }

    #[test]
    fn percent_encode_keeps_unreserved_and_slashes() {
        use super::percent_encode_path;
        assert_eq!(percent_encode_path("/home/user/file.txt"), "/home/user/file.txt");
        assert_eq!(
            percent_encode_path("/home/foo bar"),
            "/home/foo%20bar"
        );
        assert_eq!(
            percent_encode_path("/tmp/名前.txt"),
            "/tmp/%E5%90%8D%E5%89%8D.txt"
        );
    }
}
