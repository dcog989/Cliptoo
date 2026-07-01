// Content classification pipeline (Phase 1)
// Order: trim → empty → rtf → url → color → filepath → code_heuristic → text

pub mod classifier;
pub mod filetype;
pub mod hash;
pub mod preview;

pub use classifier::ContentProcessor;
pub use hash::{normalize_line_endings, sha256_hex, sha256_u64};

/// Decode percent-encoded bytes in a `file://` URI path back to a filesystem path.
pub fn percent_decode_path(s: &str) -> String {
    let mut bytes = Vec::with_capacity(s.len());
    let mut iter = s.bytes();
    while let Some(b) = iter.next() {
        if b == b'%' {
            let hi = iter
                .next()
                .and_then(|c| (c as char).to_digit(16))
                .unwrap_or(0) as u8;
            let lo = iter
                .next()
                .and_then(|c| (c as char).to_digit(16))
                .unwrap_or(0) as u8;
            bytes.push(hi * 16 + lo);
        } else {
            bytes.push(b);
        }
    }
    String::from_utf8(bytes).unwrap_or_else(|_| s.to_string())
}
