// Text transformation implementations.
// See PORTING.md §5 for per-key notes.
// All case ops use unicode-segmentation grapheme cluster iteration.

use crate::time::local_timestamp;
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
        "kebab" => normalise_to_words(content).join("-").to_lowercase(),

        // Convert to snake_case.
        "snake" => normalise_to_words(content).join("_").to_lowercase(),

        // Convert to camelCase.
        "camel" => {
            let words = normalise_to_words(content);
            words
                .iter()
                .enumerate()
                .map(|(i, w)| {
                    if i == 0 {
                        w.to_lowercase()
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

        // Convert slug / snake_case to readable words with first-word capitalisation.
        "deslug" => {
            let spaced = content.replace(['_', '-'], " ");
            capitalize_first(&spaced)
        }

        // Normalise to single newlines, collapse 3+ to \n\n.
        "lf1" => {
            let normalised = content.replace("\r\n", "\n").replace('\r', "\n");
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
            out
        }

        // Normalise then ensure all paragraph breaks become \n\n.
        "lf2" => {
            let normalised = content.replace("\r\n", "\n").replace('\r', "\n");
            // Split on blank lines (\n\n+), trim each paragraph, rejoin with \n\n.
            let paragraphs: Vec<&str> = normalised
                .split("\n\n")
                .map(|p| p.trim_matches('\n'))
                .filter(|p| !p.is_empty())
                .collect();
            paragraphs.join("\n\n")
        }

        // Strip all line breaks.
        "remove_lf" => content
            .replace("\r\n", " ")
            .replace(['\r', '\n'], " ")
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" "),

        // Prepend [YYYY-MM-DD HH:MM:SS] using local time (via std::time).
        "timestamp" => {
            let ts = local_timestamp();
            format!("{ts} {content}")
        }

        _ => content.to_string(),
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Split content on whitespace, hyphens, and underscores — returns individual
/// words suitable for reassembly into kebab / snake / camel.
fn normalise_to_words(content: &str) -> Vec<&str> {
    content
        .split(|c: char| c.is_whitespace() || c == '-' || c == '_')
        .filter(|w| !w.is_empty())
        .collect()
}

/// Capitalise the first grapheme of each sentence. Sentence boundaries are
/// defined as `.`, `!`, or `?` followed by one or more whitespace characters.
fn capitalize_sentences(content: &str) -> String {
    let mut out = String::with_capacity(content.len());
    let mut capitalise_next = true;

    let chars = content.chars();
    for ch in chars {
        if ch == '.' || ch == '!' || ch == '?' {
            out.push(ch);
            // Flag the next non-whitespace character for capitalisation.
            capitalise_next = true;
        } else if capitalise_next && !ch.is_whitespace() {
            // to_uppercase can expand to multiple chars (e.g. ß → SS).
            for uc in ch.to_uppercase() {
                out.push(uc);
            }
            capitalise_next = false;
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

    #[test]
    fn invert() {
        assert_eq!(transform("Hello World", "invert"), "hELLO wORLD");
    }

    #[test]
    fn lf1_collapses_triple_newlines() {
        assert_eq!(transform("a\n\n\nb", "lf1"), "a\n\nb");
    }

    #[test]
    fn remove_lf() {
        assert_eq!(transform("a\nb", "remove_lf"), "a b");
    }

    #[test]
    fn sentence_capitalise() {
        assert_eq!(transform("hello. world", "sentence"), "Hello. World");
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
}
