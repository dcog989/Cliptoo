use crate::db::models::ClipType;

/// Result of running a payload through the content classification pipeline.
///
/// The caller is responsible for normalizing line endings before calling
/// `ContentProcessor::process()`.  The `content_hash` and
/// `content_hash_prefix` are computed from the **trimmed** content that
/// `process` returns, so suppression and DB dedup both use the same hash.
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
    /// First 8 bytes of SHA-256 as a u64 (little-endian).
    /// Used for fast in-memory paste suppression.
    pub content_hash_prefix: u64,
}

/// Stateless content processor. Classifies incoming clipboard payloads.
///
/// Classification order (first match wins):
///   1. Whitespace trim detection
///   2. Empty / whitespace-only  → discard (returns None)
///   3. RTF detection             → ClipType::Rtf
///   4. URL detection             → ClipType::Link
///   5. Color detection           → ClipType::Color
///   6. File path detection       → ClipType::Folder | file_*
///   7. Code heuristic            → ClipType::CodeSnippet
///   8. Fallback                  → ClipType::Text
pub struct ContentProcessor;

impl ContentProcessor {
    /// Classify pre-normalized clipboard content.
    ///
    /// The caller must have called `normalize_line_endings()` on `raw` before
    /// passing it in.  This function trims whitespace, classifies, builds the
    /// preview, and computes `content_hash` + `content_hash_prefix` from the
    /// **trimmed** result so that suppression and DB dedup both agree on the
    /// same hash.
    pub fn process(normalized: &str) -> Option<ClassifiedContent> {
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
        let (clip_type, content) = if content.starts_with(r"{\rtf") {
            (ClipType::Rtf, content)
        } else if Self::is_url(&content) {
            (ClipType::Link, content)
        } else if crate::color::ColorParser::is_color(&content) {
            (ClipType::Color, content)
        } else if let Some((ft, decoded_path)) = Self::classify_path(&content) {
            (ft, decoded_path)
        } else if Self::is_code_heuristic(&content) {
            (ClipType::CodeSnippet, content)
        } else {
            (ClipType::Text, content)
        };

        let size_in_bytes = content.len() as i64;
        let is_multiline = content.contains('\n');
        let preview_content = crate::content::preview::build_preview(&content);
        let content_hash = crate::content::hash::sha256_hex(&content);
        let content_hash_prefix = crate::content::hash::sha256_u64(&content);

        Some(ClassifiedContent {
            content,
            clip_type,
            was_trimmed,
            has_leading_whitespace,
            is_multiline,
            size_in_bytes,
            preview_content,
            content_hash,
            content_hash_prefix,
        })
    }

    fn is_url(s: &str) -> bool {
        s.starts_with("http://") || s.starts_with("https://") || s.starts_with("ftp://")
    }

    /// Checks whether `s` looks like a filesystem path and exists on disk.
    ///
    /// Returns `Some((clip_type, decoded_path))` on a match, where
    /// `decoded_path` is the percent-decoded path with any `file://` prefix
    /// stripped — ready to store as `Content`. This avoids the caller having
    /// to decode and strip a second time.
    fn classify_path(s: &str) -> Option<(ClipType, String)> {
        use std::path::Path;
        let without_scheme = s.strip_prefix("file://").unwrap_or(s);
        let decoded = crate::content::percent_decode_path(without_scheme);
        let s = decoded.as_str();
        let looks_like_path = s.starts_with('/')
            || s.starts_with("~/")
            || s.starts_with("./")
            || s.starts_with("../")
            || s.len() >= 3
                && s.as_bytes()[1] == b':'
                && (s.as_bytes()[2] == b'\\' || s.as_bytes()[2] == b'/');
        if !looks_like_path {
            return None;
        }
        let path = Path::new(s);
        if path.is_dir() {
            return Some((ClipType::Folder, decoded));
        }
        if path.is_file() {
            let ft = crate::content::filetype::FileTypeClassifier::classify(path);
            return Some((ft, decoded));
        }
        None
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
        let score: usize = lines
            .iter()
            .map(|l| {
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
                if brackets || fat_arrow || keyword {
                    1
                } else {
                    0
                }
            })
            .sum();
        // Require structural tokens in at least CODE_MIN_SCORE lines AND at
        // least CODE_SCORE_PERCENT of lines, so short dense snippets still
        // qualify but long prose with two stray braces does not.
        score >= CODE_MIN_SCORE && score * 100 >= lines.len() * CODE_SCORE_PERCENT
    }
}
