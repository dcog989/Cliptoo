/// Build the PreviewContent string stored in the database.
///
/// Rules (from PORTING.md §1.4):
///   1. Replace all newlines and runs of whitespace with a single space.
///   2. Trim to a maximum of ~PREVIEW_MAX_BYTES bytes.
const PREVIEW_MAX_BYTES: usize = 200;

pub fn build_preview(content: &str) -> String {
    // Collapse all whitespace (including newlines) to single spaces, building
    // only up to the truncation point — a large paste must not be fully
    // collapsed and copied just to render a 200-byte preview.
    let mut collapsed = String::new();
    let mut pending_space = false;
    for c in content.chars() {
        if collapsed.len() > PREVIEW_MAX_BYTES {
            break;
        }
        if c.is_whitespace() {
            pending_space = true;
        } else {
            if pending_space && !collapsed.is_empty() {
                collapsed.push(' ');
            }
            collapsed.push(c);
            pending_space = false;
        }
    }

    if collapsed.len() <= PREVIEW_MAX_BYTES {
        collapsed
    } else {
        // Find the last char that fits entirely within PREVIEW_MAX_BYTES bytes.
        let byte_end = collapsed
            .char_indices()
            .take_while(|(i, c)| *i + c.len_utf8() <= PREVIEW_MAX_BYTES)
            .last()
            .map(|(i, c)| i + c.len_utf8())
            .unwrap_or(0);
        let truncated = &collapsed[..byte_end];
        match truncated.rfind(' ') {
            Some(i) => format!("{}…", &truncated[..i]),
            None => format!("{}…", truncated),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{PREVIEW_MAX_BYTES, build_preview};

    #[test]
    fn collapses_whitespace_runs() {
        assert_eq!(
            build_preview("a\n  b\t\tc\r\nd"),
            "a b c d"
        );
    }

    #[test]
    fn short_content_is_unchanged() {
        assert_eq!(build_preview("hello world"), "hello world");
    }

    #[test]
    fn truncates_at_word_boundary() {
        let words = "word ".repeat(PREVIEW_MAX_BYTES);
        let preview = build_preview(&words);
        assert!(
            preview.ends_with("word…"),
            "must cut at a word boundary, got: {preview:?}"
        );
        assert_eq!(preview.matches('…').count(), 1);
        // Stored bytes (with the ellipsis) must fit the column limit plus the marker.
        assert!(preview.len() <= PREVIEW_MAX_BYTES + 3);
    }

    #[test]
    fn truncates_long_single_word() {
        let long = "x".repeat(PREVIEW_MAX_BYTES * 2);
        let preview = build_preview(&long);
        assert!(preview.ends_with('…'));
        assert!(preview.len() <= PREVIEW_MAX_BYTES + 3);
    }
}
