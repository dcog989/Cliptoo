/// Strip HTML markup to plain text, for clip previews and the `text/plain`
/// fallback offered alongside a rich `text/html` paste.
///
/// Not a full HTML parser. It removes `<script>`/`<style>` bodies, maps
/// block-level and line-break tags to newlines, drops every other tag, decodes
/// the common character entities, and collapses blank lines. This covers the
/// fragments browsers and office suites place on the clipboard (`<!--StartFragment-->`
/// wrappers, nested `<span>`/`<div>` markup) well enough for display.
pub fn strip_html(html: &str) -> String {
    let mut out = String::with_capacity(html.len());
    let mut i = 0;
    let bytes = html.as_bytes();
    // While inside a `<script>`/`<style>` body, tag-like tokens are dropped
    // until the matching close tag is seen.
    let mut in_skipped_block: Option<&str> = None;

    while i < bytes.len() {
        if is_tag_start_at(bytes, i) {
            let start = i;
            let mut j = i + 1;
            while j < bytes.len() && bytes[j] != b'>' {
                j += 1;
            }
            let has_close = j < bytes.len();
            // `tag` spans `<` .. `>` inclusive when `>` exists, else to end.
            let tag = if has_close {
                &html[start..=j]
            } else {
                &html[start..]
            };
            let name = tag_name(tag);
            let closing = tag.as_bytes().get(1) == Some(&b'/');

            if let Some(open) = in_skipped_block {
                if closing && name.eq_ignore_ascii_case(open) {
                    in_skipped_block = None;
                }
                // Everything inside the block is dropped.
            } else if !closing
                && (name.eq_ignore_ascii_case("script") || name.eq_ignore_ascii_case("style"))
            {
                in_skipped_block = Some(name);
            } else if !name.is_empty() && BLOCK_TAGS.iter().any(|b| name.eq_ignore_ascii_case(b)) {
                out.push('\n');
            }
            // Comments, doctypes and ordinary tags contribute nothing.

            i = if has_close { j + 1 } else { bytes.len() };
            continue;
        }

        // Text run: push the current character verbatim, except inside a
        // script/style body (already handled by the tag branch's close tag).
        let c = html[i..].chars().next().expect("i is on a char boundary");
        if in_skipped_block.is_none() {
            out.push(c);
        }
        i += c.len_utf8();
    }

    collapse_blank_lines(&decode_entities(&out))
}

/// Tags after which a newline is inserted. Nested block elements naturally
/// produce blank lines; `collapse_blank_lines` folds those runs.
const BLOCK_TAGS: &[&str] = &[
    "p",
    "div",
    "br",
    "li",
    "h1",
    "h2",
    "h3",
    "h4",
    "h5",
    "h6",
    "tr",
    "blockquote",
    "pre",
    "ul",
    "ol",
    "table",
    "section",
    "header",
    "footer",
    "article",
    "aside",
    "main",
    "nav",
    "figure",
    "hr",
    "dt",
    "dd",
    "td",
    "th",
];

/// True when `i` begins a tag: `<` followed by a letter, `/`, `!` or `?`.
/// A stray `<` in text (e.g. "2 < 3") is never a tag start.
fn is_tag_start_at(bytes: &[u8], i: usize) -> bool {
    bytes[i] == b'<' && bytes.get(i + 1).is_some_and(is_tag_lead)
}

fn is_tag_lead(b: &u8) -> bool {
    b.is_ascii_alphabetic() || *b == b'/' || *b == b'!' || *b == b'?'
}

/// Extract the element name from a tag span that includes the outer `<` and
/// `>`. Returns an empty string when the span carries no element name (a
/// doctype, comment, or bare `>`).
fn tag_name(tag: &str) -> &str {
    let inner = tag.get(1..tag.len().saturating_sub(1)).unwrap_or("");
    let inner = inner.trim_start_matches('/').trim_start();
    let end = inner
        .find(|c: char| c == '/' || c == '>' || c.is_whitespace())
        .unwrap_or(inner.len());
    &inner[..end]
}

/// Decode `&...;` character references (named and numeric) in `s`.
fn decode_entities(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let mut i = 0;
    while i < s.len() {
        if s.as_bytes()[i] == b'&'
            && let Some(end) = s[i..].find(';')
        {
            let entity = &s[i..i + end + 1];
            if let Some(c) = decode_entity(entity) {
                result.push(c);
                i += end + 1;
                continue;
            }
        }
        let c = s[i..].chars().next().expect("i is on a char boundary");
        result.push(c);
        i += c.len_utf8();
    }
    result
}

fn decode_entity(entity: &str) -> Option<char> {
    let inner = entity.get(1..entity.len() - 1)?;
    if let Some(rest) = inner.strip_prefix('#') {
        if let Some(hex) = rest.strip_prefix(['x', 'X']) {
            u32::from_str_radix(hex, 16).ok().and_then(char::from_u32)
        } else {
            rest.parse::<u32>().ok().and_then(char::from_u32)
        }
    } else {
        match inner {
            "amp" => Some('&'),
            "lt" => Some('<'),
            "gt" => Some('>'),
            "quot" => Some('"'),
            "apos" => Some('\''),
            "nbsp" => Some('\u{00a0}'),
            "copy" => Some('\u{00a9}'),
            "reg" => Some('\u{00ae}'),
            "trade" => Some('\u{2122}'),
            "mdash" => Some('\u{2014}'),
            "ndash" => Some('\u{2013}'),
            "hellip" => Some('\u{2026}'),
            "lsquo" => Some('\u{2018}'),
            "rsquo" => Some('\u{2019}'),
            "ldquo" => Some('\u{201c}'),
            "rdquo" => Some('\u{201d}'),
            "middot" => Some('\u{00b7}'),
            "bull" => Some('\u{2022}'),
            "times" => Some('\u{00d7}'),
            "divide" => Some('\u{00f7}'),
            "eacute" => Some('\u{00e9}'),
            "egrave" => Some('\u{00e8}'),
            "agrave" => Some('\u{00e0}'),
            "ccedil" => Some('\u{00e7}'),
            "ntilde" => Some('\u{00f1}'),
            "uuml" => Some('\u{00fc}'),
            _ => None,
        }
    }
}

/// Collapse runs of blank lines, matching the RTF stripper's output shape so
/// both rich formats produce the same preview style.
fn collapse_blank_lines(s: &str) -> String {
    s.lines()
        .filter(|l| !l.trim().is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::strip_html;

    #[test]
    fn strips_tags_and_keeps_text() {
        assert_eq!(strip_html("<p>hello <b>world</b></p>"), "hello world");
    }

    #[test]
    fn maps_block_tags_to_newlines() {
        assert_eq!(strip_html("<div>a</div><div>b</div>"), "a\nb");
        assert_eq!(strip_html("<ul><li>one</li><li>two</li></ul>"), "one\ntwo");
        assert_eq!(strip_html("line<br>break"), "line\nbreak");
    }

    #[test]
    fn drops_script_and_style_bodies() {
        assert_eq!(
            strip_html("<p>a</p><script>if (a < b) { x(); }</script><style>.x { }</style><p>b</p>"),
            "a\nb"
        );
    }

    #[test]
    fn drops_chrome_fragment_wrapper() {
        let html =
            "<!--StartFragment--><span style=\"font-weight:bold\">Hi</span><!--EndFragment-->";
        assert_eq!(strip_html(html), "Hi");
    }

    #[test]
    fn decodes_common_entities() {
        assert_eq!(
            strip_html("<p>a &amp; b &lt;c&gt; &quot;q&quot; &#39;s&#39;</p>"),
            "a & b <c> \"q\" 's'"
        );
        assert_eq!(
            strip_html("<p>caf&#233; &#x1F600; &nbsp;x</p>"),
            "café 😀 \u{00a0}x"
        );
    }

    #[test]
    fn less_than_in_text_is_not_a_tag() {
        assert_eq!(strip_html("2 < 3 is true"), "2 < 3 is true");
    }

    #[test]
    fn malformed_entity_passes_through() {
        assert_eq!(strip_html("<p>a & b</p>"), "a & b");
        assert_eq!(strip_html("<p>fish &amp</p>"), "fish &amp");
    }
}
