/// Remove RTF markup, returning the plain-text content.
///
/// Algorithm: iterate characters, skip everything inside nested `{...}` groups
/// whose first token starts with `\\`, pass through characters that are not
/// RTF control words.  Handles the common subset produced by office
/// applications; does not attempt full RTF spec compliance.
pub fn strip_rtf(rtf: &str) -> String {
    // Accumulated as raw bytes, not `char`s: passthrough content may contain
    // multi-byte UTF-8 sequences, and pushing each byte individually via
    // `byte as char` would reinterpret every byte >= 0x80 as its own Latin-1
    // codepoint instead of reassembling the original character. Decoding once
    // at the end (via `from_utf8_lossy`) keeps multi-byte sequences intact.
    let mut out: Vec<u8> = Vec::with_capacity(rtf.len());
    let mut depth: u32 = 0;
    let mut skip_group = false;
    let mut skip_stack: Vec<bool> = Vec::new();
    let bytes = rtf.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'{' => {
                skip_stack.push(skip_group);
                // Peek: if next non-space char is '\\', this is a control group.
                let mut j = i + 1;
                while j < bytes.len() && bytes[j] == b' ' {
                    j += 1;
                }
                // Only groups *nested inside the root* count as control groups.
                // The root `{\rtf...}` group itself starts with a backslash too;
                // marking it as a control group would skip the whole document
                // body and strip every real RTF payload to an empty string.
                skip_group = depth > 0 && j < bytes.len() && bytes[j] == b'\\';
                depth += 1;
                i += 1;
            }
            b'}' => {
                depth = depth.saturating_sub(1);
                skip_group = skip_stack.pop().unwrap_or(false);
                i += 1;
            }
            b'\\' => {
                i += 1;
                // Escaped char literal (e.g. \\{ \\} \\\\)
                if i < bytes.len() && (bytes[i] == b'{' || bytes[i] == b'}' || bytes[i] == b'\\') {
                    if !skip_group && depth > 0 {
                        out.push(bytes[i]);
                    }
                    i += 1;
                    continue;
                }
                // Control word: letters then optional signed integer
                let start = i;
                while i < bytes.len() && bytes[i].is_ascii_alphabetic() {
                    i += 1;
                }
                let word = &rtf[start..i];
                // Optional numeric parameter
                let _neg = i < bytes.len() && bytes[i] == b'-';
                if _neg {
                    i += 1;
                }
                while i < bytes.len() && bytes[i].is_ascii_digit() {
                    i += 1;
                }
                // Trailing space delimiter is consumed
                if i < bytes.len() && bytes[i] == b' ' {
                    i += 1;
                }
                if !skip_group {
                    match word {
                        "par" | "line" => out.push(b'\n'),
                        "tab" => out.push(b'\t'),
                        _ => {}
                    }
                }
            }
            b'\r' | b'\n' => {
                i += 1;
            }
            ch => {
                if !skip_group && depth > 0 {
                    out.push(ch);
                }
                i += 1;
            }
        }
    }
    // Decode once, now that every passthrough byte (including multi-byte
    // UTF-8 sequences) has been accumulated intact.
    let text = String::from_utf8_lossy(&out);
    // Collapse runs of blank lines
    text.lines()
        .filter(|l| !l.trim().is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::strip_rtf;

    #[test]
    fn strips_control_words_inside_root_group() {
        assert_eq!(strip_rtf(r"{\rtf1\ansi hello \b world\b0}"), "hello world");
    }

    #[test]
    fn skips_formatting_control_groups() {
        assert_eq!(
            strip_rtf(r"{\rtf1{\colortbl;\red255\green0\blue0;}\red\cf1 text}"),
            "text"
        );
    }

    #[test]
    fn converts_par_tab_and_line() {
        assert_eq!(strip_rtf(r"{\rtf1 a\par b\tab c}"), "a\nb\tc");
    }

    #[test]
    fn passes_escaped_braces_through() {
        assert_eq!(strip_rtf(r"{\rtf1 a \{b\} c}"), "a {b} c");
    }

    #[test]
    fn collapses_blank_lines() {
        assert_eq!(strip_rtf(r"{\rtf1 a\par\par\par b}"), "a\nb");
    }
}
