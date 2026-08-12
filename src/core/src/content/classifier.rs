use crate::db::models::ClipType;

/// Result of running a payload through the content classification pipeline.
///
/// The caller is responsible for normalizing line endings before calling
/// `ContentProcessor::process()`.  The `content_hash` is computed from the
/// **trimmed** content that `process` returns, so suppression and DB dedup both
/// use the same hash.
pub struct ClassifiedContent {
    pub content: String,
    pub clip_type: ClipType,
    pub was_trimmed: bool,
    pub has_leading_whitespace: bool,
    pub is_multiline: bool,
    pub size_in_bytes: i64,
    pub preview_content: String,
    /// Full SHA-256 hex digest of trimmed `content` (64 chars).
    /// Used for DB `ContentHash` column (UNIQUE).
    pub content_hash: String,
}

/// Stateless content processor. Classifies incoming clipboard payloads.
///
/// `is_copied_file` tells the classifier whether the payload was copied as an
/// actual file/folder (via `text/uri-list`) or is plain text. This matters for
/// path detection: a path the user typed/copied as text is a `FilePath` clip,
/// while a file/folder copied from a file manager gets the `Folder`/`file_*`
/// classification.
///
/// Classification order (first match wins):
///   1. Whitespace trim detection
///   2. Empty / whitespace-only  → discard (returns None)
///   3. RTF detection             → ClipType::Rtf
///   4. URL detection             → ClipType::Link
///   5. Color detection           → ClipType::Color
///   6. Path detection:
///      copied file → ClipType::Folder | file_*
///      plain text  → ClipType::FilePath
///   7. Code heuristic            → ClipType::CodeSnippet
///   8. Fallback                  → ClipType::Text
pub struct ContentProcessor;

impl ContentProcessor {
    /// Classify pre-normalized clipboard content.
    ///
    /// The caller must have called `normalize_line_endings()` on `raw` before
    /// passing it in.  This function trims whitespace, classifies, builds the
    /// preview, and computes `content_hash` from the **trimmed** result so that
    /// suppression and DB dedup both agree on the same hash.
    pub fn process(normalized: &str, is_copied_file: bool) -> Option<ClassifiedContent> {
        // Step 1: trim detection
        let trimmed = normalized.trim();
        let was_trimmed = trimmed != normalized;
        let has_leading_whitespace = normalized.chars().next().is_some_and(char::is_whitespace);
        let content = trimmed.to_string();
        // Step 2: discard empty
        if content.is_empty() {
            return None;
        }

        // classify_path returns the decoded filesystem path alongside the type
        // so we don't have to decode/strip a second time below.
        let (clip_type, content) = if Self::is_rtf(&content) {
            (ClipType::Rtf, content)
        } else if Self::is_url(&content) {
            (ClipType::Link, content)
        } else if crate::color::ColorParser::is_color(&content) {
            (ClipType::Color, content)
        } else if is_copied_file {
            match Self::classify_path(&content) {
                Some((ft, decoded_path)) => (ft, decoded_path),
                None if Self::is_code_heuristic(&content) => (ClipType::CodeSnippet, content),
                None => (ClipType::Text, content),
            }
        } else if !content.contains('\n') && Self::looks_like_path(&content) {
            // Text that is wholly a single file path (to a folder or file).
            // Multiline content (e.g. a code block that merely contains a
            // path) never counts — only the whole clip being a path does.
            // A `file://` scheme is stripped and percent-encoding decoded so
            // the stored path matches what `classify_path` stores for a
            // copied file: `file:///home/foo%20bar` becomes `/home/foo bar`.
            let decoded = content
                .strip_prefix("file://")
                .map(crate::content::percent_decode_path);
            (ClipType::FilePath, decoded.unwrap_or(content))
        } else if Self::is_code_heuristic(&content) {
            (ClipType::CodeSnippet, content)
        } else {
            (ClipType::Text, content)
        };

        let size_in_bytes = content.len() as i64;
        let is_multiline = content.contains('\n');
        // RTF clips preview as their stripped plain text; everything else as-is.
        let preview_content = if clip_type == ClipType::Rtf {
            crate::content::preview::build_preview(&crate::content::strip_rtf(&content))
        } else {
            crate::content::preview::build_preview(&content)
        };
        let content_hash = crate::content::hash::sha256_hex(&content);

        Some(ClassifiedContent {
            content,
            clip_type,
            was_trimmed,
            has_leading_whitespace,
            is_multiline,
            size_in_bytes,
            preview_content,
            content_hash,
        })
    }

    fn is_url(s: &str) -> bool {
        // Case-insensitive scheme matching (HTTP://, Https://, ...); mailto:
        // takes a single colon and bare www. domains carry no scheme at all.
        Self::starts_with_ci(s, "http://")
            || Self::starts_with_ci(s, "https://")
            || Self::starts_with_ci(s, "ftp://")
            || Self::starts_with_ci(s, "mailto:")
            || Self::starts_with_ci(s, "www.")
    }

    /// Case-insensitive prefix match that is safe on non-ASCII content.
    fn starts_with_ci(s: &str, prefix: &str) -> bool {
        s.get(..prefix.len())
            .is_some_and(|p| p.eq_ignore_ascii_case(prefix))
    }

    /// True when `s` is an RTF document, tolerating a leading UTF-8 BOM that
    /// some Windows apps prepend to clipboard RTF.
    fn is_rtf(s: &str) -> bool {
        s.strip_prefix('\u{feff}')
            .unwrap_or(s)
            .starts_with(r"{\rtf")
    }

    /// True when `raw` looks like a filesystem path, regardless of whether it
    /// exists on disk. `file://` prefixes and percent-encoding are handled, so
    /// both `file:///home/foo%20bar` and `/home/foo bar` count. Also accepts a
    /// newline-joined list of paths (a multi-selection `text/uri-list` payload).
    pub fn looks_like_path(raw: &str) -> bool {
        let without_scheme = raw.strip_prefix("file://").unwrap_or(raw);
        let decoded = crate::content::percent_decode_path(without_scheme);
        decoded.starts_with('/')
            || decoded.starts_with("~/")
            || decoded.starts_with("./")
            || decoded.starts_with("../")
            || decoded.len() >= 3
                && decoded.as_bytes()[1] == b':'
                && (decoded.as_bytes()[2] == b'\\' || decoded.as_bytes()[2] == b'/')
    }

    /// Checks whether `s` is a filesystem path (or newline-joined path list)
    /// and classifies it.
    ///
    /// Returns `Some((clip_type, decoded_paths))` on a match, where
    /// `decoded_paths` is the percent-decoded, `file://`-stripped content —
    /// ready to store as `Content`. A single existing path gets its precise
    /// type (`Folder` or a `file_*`); a single path-like path that no longer
    /// exists falls back to `FileGeneric`. A multi-path payload (a
    /// multi-selection copy) is `Folder` only when every path is an existing
    /// directory, otherwise `FileGeneric`. Missing paths need no existence
    /// check: they become deadheads via maintenance.
    fn classify_path(s: &str) -> Option<(ClipType, String)> {
        use std::path::Path;
        if !Self::looks_like_path(s) {
            return None;
        }
        // The uri-list ingestion path already percent-decodes and strips the
        // `file://` prefix before calling `process`; only decode when the scheme
        // is still present, otherwise re-decoding would corrupt paths that
        // legitimately contain percent sequences (e.g. a file named `foo%20bar`).
        let decoded: Vec<String> = s
            .lines()
            .map(|line| match line.strip_prefix("file://") {
                Some(rest) => crate::content::percent_decode_path(rest),
                None => line.to_string(),
            })
            .collect();

        if decoded.len() == 1 {
            let path = Path::new(decoded[0].as_str());
            if path.is_dir() {
                return Some((ClipType::Folder, decoded[0].clone()));
            }
            if path.is_file() {
                let ft = crate::content::filetype::FileTypeClassifier::classify(path);
                return Some((ft, decoded[0].clone()));
            }
            // Path-like but present on disk neither as a file nor a directory
            // (deleted between the copy and the poll, or on a stale re-read).
            // Keep it a file clip so deadhead maintenance can track it, and so
            // reclassify does not downgrade a temporarily-gone path to text —
            // mirrors the multi-selection branch, which also keeps missing
            // paths as FileGeneric.
            return Some((ClipType::FileGeneric, decoded[0].clone()));
        }

        // Multi-selection copy. Reject when a line isn't path-like at all (a
        // stray non-path line shouldn't be silently classified as a file clip).
        if decoded.iter().any(|l| !Self::looks_like_path(l)) {
            return None;
        }
        let all_dirs = decoded.iter().all(|l| Path::new(l.as_str()).is_dir());
        let clip_type = if all_dirs {
            ClipType::Folder
        } else {
            ClipType::FileGeneric
        };
        Some((clip_type, decoded.join("\n")))
    }

    fn is_code_heuristic(s: &str) -> bool {
        // Coarse heuristic: multi-line content with enough structural tokens.
        // Intentionally conservative to reduce false positives on git diffs,
        // YAML, and prose.  The line minimum and score threshold are the main
        // knobs; refine per-format detection in a future pass.
        const CODE_MIN_LINES: usize = 3;
        const CODE_MIN_SCORE: usize = 2;
        // Require at least this percentage of lines to carry structural tokens.
        const CODE_SCORE_PERCENT: usize = 30;

        let lines: Vec<&str> = s.lines().collect();
        if lines.len() < CODE_MIN_LINES {
            return false;
        }
        let mut score: usize = 0;
        for (idx, line) in lines.iter().enumerate() {
            if Self::is_code_line(line) {
                score += 1;
            }
            let processed = idx + 1;
            let best_possible = score + (lines.len() - processed);
            // Both thresholds met already — score only grows from here, the
            // total line count is fixed, so the outcome cannot change.
            if score >= CODE_MIN_SCORE && score * 100 >= lines.len() * CODE_SCORE_PERCENT {
                return true;
            }
            // Even if every remaining line scored 1, a threshold is unreachable.
            if best_possible < CODE_MIN_SCORE
                || best_possible * 100 < lines.len() * CODE_SCORE_PERCENT
            {
                return false;
            }
        }
        score >= CODE_MIN_SCORE && score * 100 >= lines.len() * CODE_SCORE_PERCENT
    }

    fn is_code_line(l: &str) -> bool {
        // Bracket tokens: hard structural evidence
        let brackets = l.contains('{') || l.contains('}');
        // Fat arrow only when followed by something (not bare YAML `key: value =>`)
        let fat_arrow = l.contains("=> ") || l.ends_with("=>");
        // Language keywords as whole words (not substrings of identifiers)
        let keyword = l.contains(" fn ")
            || l.starts_with("fn ")
            || l.contains(" def ")
            || l.starts_with("def ")
            || l.contains(" func ")
            || l.starts_with("func ")
            || l.contains(" class ")
            || l.starts_with("class ")
            || l.contains(" return ")
            || l.contains(" return;")
            || l.contains(" import ")
            || l.starts_with("import ")
            || l.contains(" pub ")
            || l.contains(" let ")
            || l.contains(" const ");
        brackets || fat_arrow || keyword
    }
}

#[cfg(test)]
mod tests {
    use super::ContentProcessor;
    use crate::db::models::ClipType;

    fn temp_dir(name: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "cliptoo_classifier_{}_{}",
            std::process::id(),
            name
        ))
    }

    #[test]
    fn single_existing_file_gets_specific_type() {
        let dir = temp_dir("single");
        let path = dir.join("sample.png");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(&path, b"x").unwrap();
        let c = ContentProcessor::process(path.to_str().unwrap(), true).unwrap();
        assert_eq!(c.clip_type, ClipType::FileImage);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn multi_file_selection_is_generic_file() {
        let dir = temp_dir("multi");
        std::fs::create_dir_all(&dir).unwrap();
        let a = dir.join("a.txt");
        let b = dir.join("b.md");
        std::fs::write(&a, b"x").unwrap();
        std::fs::write(&b, b"x").unwrap();
        let content = format!("{}\n{}", a.display(), b.display());
        let c = ContentProcessor::process(&content, true).unwrap();
        assert_eq!(c.clip_type, ClipType::FileGeneric);
        assert!(c.is_multiline);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn multi_selection_all_dirs_is_folder() {
        let dir = temp_dir("dirs");
        let d1 = dir.join("one");
        let d2 = dir.join("two");
        std::fs::create_dir_all(&d1).unwrap();
        std::fs::create_dir_all(&d2).unwrap();
        let content = format!("{}\n{}", d1.display(), d2.display());
        let c = ContentProcessor::process(&content, true).unwrap();
        assert_eq!(c.clip_type, ClipType::Folder);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn multi_selection_missing_paths_stay_file_clips() {
        // A path that vanished is not a reason to reclassify a multi-selection
        // as text — deadhead maintenance handles the missing paths later.
        let dir = temp_dir("missing");
        let a = dir.join("a.txt");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(&a, b"x").unwrap();
        let gone = dir.join("gone.png");
        let content = format!("{}\n{}", a.display(), gone.display());
        let c = ContentProcessor::process(&content, true).unwrap();
        assert_eq!(c.clip_type, ClipType::FileGeneric);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn single_missing_path_stays_file_clip() {
        // A single copied path whose file has vanished since the copy must
        // stay a file clip (FileGeneric) so deadhead maintenance can track it,
        // matching the multi-selection branch instead of degrading to text.
        let dir = temp_dir("missing_single");
        let gone = dir.join("gone.png");
        let c = ContentProcessor::process(gone.to_str().unwrap(), true).unwrap();
        assert_eq!(c.clip_type, ClipType::FileGeneric);
        assert!(!c.is_multiline);
    }

    #[test]
    fn single_non_path_text_stays_text() {
        let c = ContentProcessor::process("not a path at all", true).unwrap();
        assert_eq!(c.clip_type, ClipType::Text);
    }

    #[test]
    fn plain_single_path_text_is_file_path() {
        let c = ContentProcessor::process("/home/user/foo.txt", false).unwrap();
        assert_eq!(c.clip_type, ClipType::FilePath);
        assert_eq!(c.content, "/home/user/foo.txt");
    }

    #[test]
    fn file_uri_text_is_decoded_file_path() {
        // `file://` + percent-encoding is normalised like a copied file's
        // path, so the stored path is the real filesystem path, not the URI.
        let c = ContentProcessor::process("file:///home/user/foo%20bar.txt", false).unwrap();
        assert_eq!(c.clip_type, ClipType::FilePath);
        assert_eq!(c.content, "/home/user/foo bar.txt");
    }

    #[test]
    fn dense_code_is_code_snippet() {
        let code = "fn main() {\n    let x = 1;\n    return x;\n}";
        let c = ContentProcessor::process(code, false).unwrap();
        assert_eq!(c.clip_type, ClipType::CodeSnippet);
    }

    #[test]
    fn long_prose_without_tokens_stays_text() {
        // 40 lines of plain prose must not trip the code heuristic, exercising
        // the percent-threshold early-exit in is_code_heuristic.
        let prose = (0..40)
            .map(|i| format!("this is ordinary sentence number {i} with no brackets"))
            .collect::<Vec<_>>()
            .join("\n");
        let c = ContentProcessor::process(&prose, false).unwrap();
        assert_eq!(c.clip_type, ClipType::Text);
    }

    #[test]
    fn three_line_code_is_code_snippet() {
        // Dense short snippet satisfies the min-line + min-score thresholds.
        let code = "if (a) {\n  b();\n  c();\n}";
        let c = ContentProcessor::process(code, false).unwrap();
        assert_eq!(c.clip_type, ClipType::CodeSnippet);
    }

    #[test]
    fn rtf_without_bom_is_rtf() {
        let c = ContentProcessor::process(r"{\rtf1\ansi hello}", false).unwrap();
        assert_eq!(c.clip_type, ClipType::Rtf);
        assert_eq!(c.preview_content, "hello");
    }

    #[test]
    fn rtf_with_bom_is_rtf() {
        let c = ContentProcessor::process("\u{feff}{\\rtf1\\ansi hello}", false).unwrap();
        assert_eq!(c.clip_type, ClipType::Rtf);
        assert_eq!(c.preview_content, "hello");
    }

    #[test]
    fn urls_are_links() {
        for url in [
            "http://example.com",
            "https://example.com",
            "ftp://example.com",
            "HTTP://EXAMPLE.COM",
            "Https://example.com",
            "mailto:user@example.com",
            "MAILTO:user@example.com",
            "www.example.com",
            "WWW.EXAMPLE.COM",
        ] {
            let c = ContentProcessor::process(url, false).unwrap();
            assert_eq!(c.clip_type, ClipType::Link, "for {url}");
        }
    }

    #[test]
    fn url_lookalikes_stay_text() {
        for s in [
            "example.com",
            "httpx://example.com",
            "mailto",
            "www",
            "wwwish.com",
            "not a url",
        ] {
            let c = ContentProcessor::process(s, false).unwrap();
            assert_eq!(c.clip_type, ClipType::Text, "for {s}");
        }
    }
}
