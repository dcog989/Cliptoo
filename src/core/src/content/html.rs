/// Strip HTML markup to plain text, for clip previews and the `text/plain`
/// fallback offered alongside a rich `text/html` paste.
///
/// Not a full HTML parser. It removes `<script>`/`<style>` bodies, maps
/// block-level tags to newlines (except ones declared `display: inline`),
/// maps line-break tags (`<br>`, `<hr>`) to newlines, drops every other tag,
/// decodes the common character entities, and collapses blank lines. This
/// covers the fragments browsers and office suites place on the clipboard
/// (`<!--StartFragment-->` wrappers, nested `<span>`/`<div>` markup) well
/// enough for display.
pub fn strip_html(html: &str) -> String {
    let mut out = String::with_capacity(html.len());
    let mut i = 0;
    let bytes = html.as_bytes();
    // While inside a `<script>`/`<style>` body, tag-like tokens are dropped
    // until the matching close tag is seen.
    let mut in_skipped_block: Option<&str> = None;
    // Open block tags with their inline-display flag; the matching close tag
    // ends the line unless the open was inline.
    let mut block_stack: Vec<bool> = Vec::new();

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
            } else if !name.is_empty() && is_void_block_tag(name) {
                // Void elements end the line at the opening tag.
                out.push('\n');
            } else if !name.is_empty() && is_block_tag(name) {
                if closing {
                    // A block element's close ends the line, unless the
                    // matching opening tag declared an inline display.
                    if !block_stack.pop().is_some_and(|inline| inline) {
                        out.push('\n');
                    }
                } else {
                    // Track inline display so the matching close knows.
                    let inline = is_inline_display(tag);
                    block_stack.push(inline);
                    if !inline {
                        out.push('\n');
                    }
                }
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

/// Block elements whose close tag ends the line. Nested block elements
/// naturally produce blank lines; `collapse_blank_lines` folds those runs.
/// `<br>`/`<hr>` are void elements handled separately.
const BLOCK_TAGS: &[&str] = &[
    "p",
    "div",
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
    "dt",
    "dd",
    "td",
    "th",
];

/// Void elements that represent an explicit line break.
const VOID_BLOCK_TAGS: &[&str] = &["br", "hr"];

fn is_block_tag(name: &str) -> bool {
    BLOCK_TAGS.iter().any(|b| name.eq_ignore_ascii_case(b))
}

fn is_void_block_tag(name: &str) -> bool {
    VOID_BLOCK_TAGS.iter().any(|b| name.eq_ignore_ascii_case(b))
}

/// True when the tag's `style` attribute sets `display: inline` or
/// `display: inline-block`. Chromium wraps inline fragments (terminal
/// colours, editor widgets) in `<div>`s carrying this style; those divs must
/// not force a block line break.
fn is_inline_display(tag: &str) -> bool {
    let Some(style) = attribute_value(tag, "style") else {
        return false;
    };
    style.split(';').any(|declaration| {
        let Some((property, value)) = declaration.split_once(':') else {
            return false;
        };
        let property = property.trim();
        let value = value.split_whitespace().next().unwrap_or("");
        property.eq_ignore_ascii_case("display")
            && (value.eq_ignore_ascii_case("inline") || value.eq_ignore_ascii_case("inline-block"))
    })
}

/// Extract the value of the `name` attribute from a tag span (outer `<`/`>`
/// included), or `None`. Attribute names are matched case-insensitively as
/// HTML requires.
fn attribute_value<'a>(tag: &'a str, name: &str) -> Option<&'a str> {
    let inner = tag.get(1..tag.len().saturating_sub(1))?;
    let bytes = inner.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        while i < bytes.len() && bytes[i].is_ascii_whitespace() {
            i += 1;
        }
        let name_start = i;
        while i < bytes.len() && !bytes[i].is_ascii_whitespace() && bytes[i] != b'=' {
            i += 1;
        }
        if inner[name_start..i].eq_ignore_ascii_case(name) {
            while i < bytes.len() && bytes[i].is_ascii_whitespace() {
                i += 1;
            }
            if bytes.get(i) == Some(&b'=') {
                i += 1;
                while i < bytes.len() && bytes[i].is_ascii_whitespace() {
                    i += 1;
                }
                let quote = bytes.get(i).copied();
                if let Some(q @ (b'"' | b'\'')) = quote {
                    i += 1;
                    let value_start = i;
                    while i < bytes.len() && bytes[i] != q {
                        i += 1;
                    }
                    return Some(&inner[value_start..i]);
                }
                let value_start = i;
                while i < bytes.len() && !bytes[i].is_ascii_whitespace() {
                    i += 1;
                }
                return Some(&inner[value_start..i]);
            }
        }
        while i < bytes.len() && !bytes[i].is_ascii_whitespace() {
            i += 1;
        }
    }
    None
}

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
    fn inline_display_divs_do_not_break_lines() {
        // Chromium wraps inline fragments (terminal colours, editor widgets)
        // in `<div style="display: inline">`; those must not insert newlines.
        let html = concat!(
            "<div style=\"font-family: monospace; white-space: pre;\">uv ",
            "<div style=\"display: inline;color: rgb(23, 185, 196);\">run</div> ",
            "<div style=\"display: inline;color: rgb(23, 185, 196);\">dicmerge</div> ",
            "<div style=\"display: inline;color: rgb(23, 185, 196);\">--write-back</div> ",
            "<div style=\"display: inline;color: rgb(23, 185, 196);\">--dry-run</div></div>"
        );
        assert_eq!(strip_html(html), "uv run dicmerge --write-back --dry-run");
    }

    #[test]
    fn inline_block_display_is_inline() {
        assert_eq!(
            strip_html(
                "<div style=\"display: inline-block\">a</div><div style=\"display: inline-block\">b</div>"
            ),
            "ab"
        );
    }

    #[test]
    fn inline_display_divs_inside_block_still_end_lines() {
        // A line div containing inline fragments still ends its own line.
        let html = concat!(
            "<div>uv <div style=\"display: inline;\">run</div></div>",
            "<div>next</div>"
        );
        assert_eq!(strip_html(html), "uv run\nnext");
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
