/// Build the PreviewContent string stored in the database.
///
/// Rules (from PORTING.md §1.4):
///   1. Replace all newlines and runs of whitespace with a single space.
///   2. Trim to a maximum of ~PREVIEW_MAX_BYTES characters.
const PREVIEW_MAX_BYTES: usize = 200;

pub fn build_preview(content: &str) -> String {
    // Collapse all whitespace (including newlines) to single spaces
    let collapsed: String = content.split_whitespace().collect::<Vec<_>>().join(" ");

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
