// Text transformation implementations.
// See PORTING.md §5 for per-key notes.
// All case ops use unicode-segmentation grapheme cluster iteration.

use chrono::Local;
use unicode_segmentation::UnicodeSegmentation;

pub fn transform(content: &str, key: &str) -> String {
    match key {
        "upper" => content.to_uppercase(),
        "lower" => content.to_lowercase(),
        "trim" => content.trim().to_string(),

        // Capitalise first grapheme of every Unicode word, preserving all
        // inter-word characters (tabs, multiple spaces, newlines, punctuation)
        // exactly as they appear in the input.
        "capitalize" => {
            let mut out = String::with_capacity(content.len());
            let mut last_end = 0usize;
            for (start, word) in content.unicode_word_indices() {
                // Copy whatever sits between the previous word and this one
                // (whitespace, punctuation, etc.) verbatim.
                out.push_str(&content[last_end..start]);
                // Capitalise the first grapheme; copy the rest unchanged.
                let mut graphemes = word.graphemes(true);
                if let Some(first) = graphemes.next() {
                    out.push_str(&first.to_uppercase());
                    out.push_str(graphemes.as_str());
                }
                last_end = start + word.len();
            }
            // Copy any trailing non-word characters (e.g. trailing punctuation).
            out.push_str(&content[last_end..]);
            out
        }

        // Capitalise first grapheme of each sentence (split on . ! ? + whitespace).
        "sentence" => capitalize_sentences(content),

        // Invert case per grapheme.
        "invert" => invert_case(content),

        // Convert to kebab-case.
        "kebab" => machine_words(content).join("-"),

        // Convert to snake_case.
        "snake" => machine_words(content).join("_"),

        // Convert to camelCase.
        "camel" => {
            let words = machine_words(content);
            words
                .iter()
                .enumerate()
                .map(|(i, w)| {
                    if i == 0 {
                        w.clone()
                    } else {
                        let mut g = w.graphemes(true);
                        match g.next() {
                            Some(first) => format!("{}{}", first.to_uppercase(), g.as_str()),
                            None => String::new(),
                        }
                    }
                })
                .collect()
        }

        // Convert machine-formatted identifiers — snake_case, kebab-case,
        // SCREAMING_SNAKE, camelCase, PascalCase, dot.case — to readable text
        // ("parseXMLResponse_v2" → "Parse xml response v2").
        "deslug" => capitalize_first(&machine_words(content).join(" ")),

        // Normalise: strip a UTF-8 BOM, LF line endings, collapse 3+ newlines
        // to \n\n, trim the clip's outer whitespace.
        "lf1" => {
            let normalised = content
                .strip_prefix('\u{feff}')
                .unwrap_or(content)
                .replace("\r\n", "\n")
                .replace('\r', "\n");
            // Collapse runs of 3+ newlines to \n\n.
            let mut out = String::with_capacity(normalised.len());
            let mut nl_run = 0usize;
            for ch in normalised.chars() {
                if ch == '\n' {
                    nl_run += 1;
                    if nl_run <= 2 {
                        out.push('\n');
                    }
                } else {
                    nl_run = 0;
                    out.push(ch);
                }
            }
            out.trim().to_string()
        }

        // Strip all line breaks, keeping whitespace inside each line.
        "remove_lf" => content.replace("\r\n", " ").replace(['\r', '\n'], " "),

        // Prepend [YYYY-MM-DD HH:MM:SS] using local time.
        "timestamp" => {
            format!("[{}] {content}", Local::now().format("%Y-%m-%d %H:%M:%S"))
        }

        _ => content.to_string(),
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Split machine-formatted identifiers into lowercase words: on whitespace and
/// `_` `-` `.` separators, plus case boundaries inside each segment —
/// lowercase/digit→uppercase (`helloWorld`) and acronym-run→word
/// (`HTTPServer`, `XMLHttpRequest`). A run followed by a single trailing
/// lowercase letter stays intact so plural acronyms ("URLs", "IDs") survive.
fn machine_words(content: &str) -> Vec<String> {
    let mut words: Vec<String> = Vec::new();
    for segment in content.split(|c: char| c.is_whitespace() || matches!(c, '_' | '-' | '.')) {
        let chars: Vec<char> = segment.chars().collect();
        let mut word = String::with_capacity(segment.len());
        for (i, &ch) in chars.iter().enumerate() {
            let prev_upper = i > 0 && chars[i - 1].is_uppercase();
            let lower_tail = matches!(chars.get(i + 1), Some(next) if next.is_lowercase())
                && matches!(chars.get(i + 2), Some(next) if next.is_lowercase());
            // An uppercase char starts a new word when it does not continue an
            // acronym run: the previous char is not uppercase, or the run ends
            // here because a multi-char lowercase word follows.
            if ch.is_uppercase() && !word.is_empty() && (!prev_upper || lower_tail) {
                words.push(std::mem::take(&mut word).to_lowercase());
            }
            word.push(ch);
        }
        if !word.is_empty() {
            words.push(word.to_lowercase());
        }
    }
    words
}

/// Capitalise the first grapheme of each sentence. A sentence boundary is `.`,
/// `!`, or `?` followed by whitespace or the end of the text, so dots inside
/// domains, decimals, and abbreviations do not start a new sentence.
fn capitalize_sentences(content: &str) -> String {
    let mut out = String::with_capacity(content.len());
    let mut capitalise_next = true;

    let mut chars = content.chars().peekable();
    while let Some(ch) = chars.next() {
        if capitalise_next && !ch.is_whitespace() {
            // to_uppercase can expand to multiple chars (e.g. ß → SS).
            for uc in ch.to_uppercase() {
                out.push(uc);
            }
            capitalise_next = false;
        } else if matches!(ch, '.' | '!' | '?')
            && chars.peek().is_none_or(|next| next.is_whitespace())
        {
            out.push(ch);
            capitalise_next = true;
        } else {
            out.push(ch);
        }
    }
    out
}

/// Capitalise only the first grapheme of the string. Leaves the rest unchanged.
fn capitalize_first(content: &str) -> String {
    let mut out = String::with_capacity(content.len());
    for g in content.graphemes(true) {
        if out.is_empty() {
            out.push_str(&g.to_uppercase());
        } else {
            out.push_str(g);
        }
    }
    out
}

/// Invert case per grapheme cluster.
fn invert_case(content: &str) -> String {
    let mut out = String::with_capacity(content.len());
    for g in content.graphemes(true) {
        // A grapheme may be multiple chars; check the first char for case.
        if let Some(first) = g.chars().next() {
            if first.is_uppercase() {
                out.push_str(&g.to_lowercase());
            } else if first.is_lowercase() {
                out.push_str(&g.to_uppercase());
            } else {
                out.push_str(g);
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn upper_lower() {
        assert_eq!(transform("hello world", "upper"), "HELLO WORLD");
        assert_eq!(transform("HELLO WORLD", "lower"), "hello world");
    }

    #[test]
    fn trim_whitespace() {
        assert_eq!(transform("  hi  ", "trim"), "hi");
    }

    #[test]
    fn kebab_snake_camel() {
        assert_eq!(transform("hello world", "kebab"), "hello-world");
        assert_eq!(transform("hello world", "snake"), "hello_world");
        assert_eq!(transform("hello world foo", "camel"), "helloWorldFoo");
    }

    #[test]
    fn deslug() {
        assert_eq!(transform("hello_world", "deslug"), "Hello world");
    }

    /// De-slug covers every common machine format: separators, camel/Pascal
    /// boundaries, SCREAMING_SNAKE, dot.case, acronym runs, plural acronyms,
    /// digits, and already-readable text (idempotent).
    #[test]
    fn deslug_machine_formats() {
        assert_eq!(transform("hello-world", "deslug"), "Hello world");
        assert_eq!(transform("helloWorld", "deslug"), "Hello world");
        assert_eq!(transform("HelloWorld", "deslug"), "Hello world");
        assert_eq!(transform("HELLO_WORLD", "deslug"), "Hello world");
        assert_eq!(transform("dot.case.text", "deslug"), "Dot case text");
        assert_eq!(transform("HTTPServer", "deslug"), "Http server");
        assert_eq!(transform("XMLHttpRequest", "deslug"), "Xml http request");
        assert_eq!(
            transform("parseXMLResponse_v2", "deslug"),
            "Parse xml response v2"
        );
        assert_eq!(transform("saveIDsAndURLs", "deslug"), "Save ids and urls");
        assert_eq!(transform("Get_URLs", "deslug"), "Get urls");
        assert_eq!(transform("HTML5Parser", "deslug"), "Html5 parser");
        assert_eq!(transform("Hello world", "deslug"), "Hello world");
        assert_eq!(transform("", "deslug"), "");
    }

    #[test]
    fn invert() {
        assert_eq!(transform("Hello World", "invert"), "hELLO wORLD");
    }

    #[test]
    fn lf1_collapses_triple_newlines() {
        assert_eq!(transform("a\n\n\nb", "lf1"), "a\n\nb");
    }

    #[test]
    fn lf1_strips_bom_and_outer_whitespace() {
        assert_eq!(transform("\u{feff}  a\r\nb  \n", "lf1"), "a\nb");
    }

    #[test]
    fn remove_lf() {
        assert_eq!(transform("a\nb", "remove_lf"), "a b");
    }

    /// Only line breaks are replaced; spacing inside a line is left alone.
    #[test]
    fn remove_lf_keeps_spacing_inside_lines() {
        assert_eq!(transform("a  b", "remove_lf"), "a  b");
        assert_eq!(
            transform("col1\tcol2\nnext", "remove_lf"),
            "col1\tcol2 next"
        );
    }

    #[test]
    fn sentence_capitalise() {
        assert_eq!(transform("hello. world", "sentence"), "Hello. World");
    }

    /// Dots inside domains, decimals, and abbreviations are not sentence ends.
    #[test]
    fn sentence_ignores_dots_inside_words() {
        assert_eq!(
            transform("visit example.com, then stop", "sentence"),
            "Visit example.com, then stop"
        );
        assert_eq!(transform("took 3.5 hours", "sentence"), "Took 3.5 hours");
    }

    /// kebab/snake/camel split camelCase and acronym runs like de-slug does.
    #[test]
    fn kebab_snake_split_case_boundaries() {
        assert_eq!(transform("helloWorld", "snake"), "hello_world");
        assert_eq!(transform("HTTPServer", "kebab"), "http-server");
        assert_eq!(transform("parseXMLResponse", "camel"), "parseXmlResponse");
    }

    /// Every key offered by FormatMenu.slint must actually change the text; a
    /// key typo would otherwise fall through and paste the clip unchanged.
    #[test]
    fn format_menu_keys_all_transform() {
        let input = " hello WORLD\nagain ";
        for key in [
            "upper",
            "lower",
            "capitalize",
            "sentence",
            "invert",
            "kebab",
            "snake",
            "camel",
            "deslug",
            "trim",
            "remove_lf",
            "lf1",
            "timestamp",
        ] {
            assert_ne!(transform(input, key), input, "key {key} is a no-op");
        }
    }

    #[test]
    fn unknown_key_passthrough() {
        assert_eq!(transform("x", "unknown_key"), "x");
    }

    #[test]
    fn capitalize_preserves_separators() {
        // Single space — baseline.
        assert_eq!(transform("hello world", "capitalize"), "Hello World");
        // Multiple spaces must be preserved, not collapsed to one.
        assert_eq!(transform("hello  world", "capitalize"), "Hello  World");
        // Tab separator.
        assert_eq!(transform("hello\tworld", "capitalize"), "Hello\tWorld");
        // Newline separator.
        assert_eq!(transform("hello\nworld", "capitalize"), "Hello\nWorld");
        // Leading/trailing punctuation preserved.
        assert_eq!(transform("'hello world'", "capitalize"), "'Hello World'");
        // Empty string.
        assert_eq!(transform("", "capitalize"), "");
    }

    /// `ß` uppercases to two characters ("SS") per Unicode case mapping.
    #[test]
    fn upper_expands_sharp_s() {
        assert_eq!(transform("straße", "upper"), "STRASSE");
    }

    /// A base letter plus a combining mark is one grapheme; capitalising it
    /// must uppercase the whole cluster, not just the base char.
    #[test]
    fn capitalize_handles_combining_marks() {
        assert_eq!(transform("e\u{301}cole", "capitalize"), "E\u{301}cole");
    }

    /// Non-cased graphemes (emoji, symbols) are left untouched by invert; a
    /// lowercase `ß` still expands to "SS".
    #[test]
    fn invert_handles_emoji_and_expands_sharp_s() {
        assert_eq!(transform("Hello \u{1F600}", "invert"), "hELLO \u{1F600}");
        assert_eq!(transform("stra\u{00DF}e", "invert"), "STRASSE");
    }

    #[test]
    fn camel_handles_empty_and_single_word() {
        assert_eq!(transform("", "camel"), "");
        assert_eq!(transform("hello", "camel"), "hello");
        assert_eq!(transform("HELLO", "camel"), "hello");
    }

    /// Later all-caps words keep only their first capital, not the whole run.
    #[test]
    fn camel_lowercases_tail_of_later_words() {
        assert_eq!(transform("USER NAME", "camel"), "userName");
        assert_eq!(transform("Hello WORLD", "camel"), "helloWorld");
    }

    /// Multi-byte content survives line-break removal intact.
    #[test]
    fn remove_lf_handles_multibyte_content() {
        assert_eq!(transform("a\n\u{1F600}\nb", "remove_lf"), "a \u{1F600} b");
    }
}
