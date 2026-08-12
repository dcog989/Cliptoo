/// Remove RTF markup, returning the plain-text content.
///
/// Algorithm: iterate characters, skip everything inside nested `{...}` groups
/// whose first token starts with `\\`, pass through characters that are not
/// RTF control words. Handles the common subset produced by office
/// applications — including `\uN` Unicode escapes and `\'hh` code-page bytes
/// (decoded as Windows-1252, the default `\ansi` code page) — but does not
/// attempt full RTF spec compliance.
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
    // `\uN` values are UTF-16 code units; a high surrogate awaits its low
    // partner before the pair is emitted as a single code point.
    let mut pending_high: Option<u16> = None;
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
                // Hex escape `\'hh`: a single byte in the ANSI code page,
                // decoded as Windows-1252 (the default `\ansi` code page).
                // Malformed `\'` without two hex digits falls through as a
                // literal quote so content is never silently lost.
                if i < bytes.len() && bytes[i] == b'\'' {
                    i += 1;
                    if let (Some(hi), Some(lo)) = (
                        crate::content::hex_val(bytes.get(i)),
                        crate::content::hex_val(bytes.get(i + 1)),
                    ) {
                        if !skip_group && depth > 0 {
                            push_codepoint(&mut out, cp1252(hi * 16 + lo));
                        }
                        i += 2;
                    } else if !skip_group && depth > 0 {
                        out.push(b'\'');
                    }
                    continue;
                }
                // Control word: letters then optional signed integer
                let start = i;
                while i < bytes.len() && bytes[i].is_ascii_alphabetic() {
                    i += 1;
                }
                let word = &rtf[start..i];
                // Optional numeric parameter. Negative for `\uN` code units
                // above 0x7FFF, which are stored as signed 16-bit values.
                let negative = i < bytes.len() && bytes[i] == b'-';
                if negative {
                    i += 1;
                }
                let num_start = i;
                while i < bytes.len() && bytes[i].is_ascii_digit() {
                    i += 1;
                }
                let number: i64 = rtf[num_start..i].parse().unwrap_or(0);
                // Trailing space delimiter is consumed
                if i < bytes.len() && bytes[i] == b' ' {
                    i += 1;
                }
                if !skip_group {
                    match word {
                        "par" | "line" => out.push(b'\n'),
                        "tab" => out.push(b'\t'),
                        "u" if num_start < i => {
                            let unit = (if negative { number + 65536 } else { number }) as u16;
                            push_unicode_unit(&mut out, &mut pending_high, unit);
                            // `\uN` is followed by a short fallback for apps
                            // without Unicode support (usually `?`). Skip it —
                            // unless the escape is terminated by a control
                            // word, in which case there is no fallback text.
                            if i < bytes.len()
                                && bytes[i] != b'\\'
                                && bytes[i] != b'{'
                                && bytes[i] != b'}'
                            {
                                i += utf8_len(bytes[i]);
                            }
                        }
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

/// Number of bytes in the UTF-8 sequence starting with `b`.
fn utf8_len(b: u8) -> usize {
    match b {
        0x00..=0x7F => 1,
        0xC0..=0xDF => 2,
        0xE0..=0xEF => 3,
        _ => 4,
    }
}

/// Append the UTF-8 encoding of a Unicode code point, dropping invalid ones.
fn push_codepoint(out: &mut Vec<u8>, cp: u32) {
    if let Some(c) = char::from_u32(cp) {
        let mut buf = [0u8; 4];
        out.extend_from_slice(c.encode_utf8(&mut buf).as_bytes());
    }
}

/// Emit one UTF-16 code unit from `\uN`, combining surrogate pairs.
fn push_unicode_unit(out: &mut Vec<u8>, pending_high: &mut Option<u16>, unit: u16) {
    if let Some(hi) = pending_high.take()
        && (0xDC00..=0xDFFF).contains(&unit)
    {
        let cp = 0x1_0000 + ((u32::from(hi) - 0xD800) << 10) + (u32::from(unit) - 0xDC00);
        push_codepoint(out, cp);
        return;
    }
    // A high surrogate not followed by a low one is dropped; the current
    // unit is then processed as an independent value.
    if (0xD800..=0xDBFF).contains(&unit) {
        *pending_high = Some(unit);
    } else if !(0xDC00..=0xDFFF).contains(&unit) {
        push_codepoint(out, u32::from(unit));
    }
    // Unpaired low surrogates are dropped.
}

/// Windows-1252 → Unicode for bytes 0x80–0x9F, the range where CP-1252
/// differs from Latin-1. All other bytes map to their own code point.
fn cp1252(byte: u8) -> u32 {
    const CP1252_SPECIAL: [u32; 32] = [
        0x20AC, 0x0081, 0x201A, 0x0192, 0x201E, 0x2026, 0x2020, 0x2021, 0x02C6, 0x2030, 0x0160,
        0x2039, 0x0152, 0x008D, 0x017D, 0x008F, 0x0090, 0x2018, 0x2019, 0x201C, 0x201D, 0x2022,
        0x2013, 0x2014, 0x02DC, 0x2122, 0x0161, 0x203A, 0x0153, 0x009D, 0x017E, 0x0178,
    ];
    if (0x80..=0x9F).contains(&byte) {
        CP1252_SPECIAL[usize::from(byte - 0x80)]
    } else {
        u32::from(byte)
    }
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

    #[test]
    fn decodes_unicode_escapes() {
        assert_eq!(strip_rtf(r"{\rtf1 \u8364?}"), "€");
        assert_eq!(strip_rtf(r"{\rtf1 \u8364 ?}"), "€");
        assert_eq!(strip_rtf(r"{\rtf1 caf\u233?}"), "café");
    }

    #[test]
    fn combines_surrogate_pairs() {
        assert_eq!(strip_rtf(r"{\rtf1 \u55357?\u56832?}"), "😀");
    }

    #[test]
    fn decodes_code_page_bytes() {
        assert_eq!(strip_rtf(r"{\rtf1 caf\'e9}"), "café");
        assert_eq!(strip_rtf(r"{\rtf1 na\'efve}"), "naïve");
        assert_eq!(strip_rtf(r"{\rtf1 \'93quoted\'94}"), "“quoted”");
        assert_eq!(strip_rtf(r"{\rtf1 \'80}"), "€");
    }

    #[test]
    fn unicode_escape_terminated_by_control_word_keeps_it() {
        // The `\b` must not be eaten as the `\u` fallback text.
        assert_eq!(strip_rtf(r"{\rtf1 a\u233?\b b}"), "aé b");
    }

    #[test]
    fn malformed_hex_escape_passes_through() {
        assert_eq!(strip_rtf(r"{\rtf1 \'zz}"), "'zz");
    }
}
