use std::sync::Arc;

use slint::ComponentHandle;

/// Parse an FTS5 match-context string with `[HL]...[/HL]` sentinels
/// into a sequence of `MatchSpan` structs for inline highlighting.
///
/// E.g. `"foo [HL]bar[/HL] baz"` →
///   `[("foo ", false), ("bar", true), (" baz", false)]`
pub fn parse_match_spans(context: &str) -> slint::ModelRc<crate::MatchSpan> {
    use cliptoo_core::db::queries::{FTS_HL_CLOSE, FTS_HL_OPEN};

    // FTS5 snippets preserve the original newlines, but the row renders on a
    // single line. Collapse all whitespace to single spaces (matching how
    // PreviewContent is built) so multi-line clips don't split across rows.
    let normalized = context.split_whitespace().collect::<Vec<_>>().join(" ");

    let mut spans: Vec<crate::MatchSpan> = Vec::new();
    let mut rest = normalized.as_str();
    while !rest.is_empty() {
        if let Some(hl_start) = rest.find(FTS_HL_OPEN) {
            if hl_start > 0 {
                spans.push(crate::MatchSpan {
                    text: rest[..hl_start].into(),
                    is_highlight: false,
                });
            }
            let after_open = &rest[hl_start + FTS_HL_OPEN.len()..];
            if let Some(hl_end) = after_open.find(FTS_HL_CLOSE) {
                spans.push(crate::MatchSpan {
                    text: after_open[..hl_end].into(),
                    is_highlight: true,
                });
                rest = &after_open[hl_end + FTS_HL_CLOSE.len()..];
            } else {
                // Unclosed [HL] — treat remainder as plain
                spans.push(crate::MatchSpan {
                    text: after_open.into(),
                    is_highlight: false,
                });
                break;
            }
        } else {
            spans.push(crate::MatchSpan {
                text: rest.into(),
                is_highlight: false,
            });
            break;
        }
    }
    slint::ModelRc::from(std::rc::Rc::new(slint::VecModel::from(spans)))
}

pub fn setup_search(
    ui: &crate::AppWindow,
    db: &Arc<cliptoo_core::db::DbPool>,
    dirs: &crate::app_dirs::AppDirs,
    tag_prefix: &str,
) {
    let search_db = db.clone();
    let search_ui = ui.as_weak();
    let search_td = dirs.thumbnails_dir.clone();
    let search_fd = dirs.favicons_dir.clone();
    let tag_prefix = tag_prefix.to_string();
    ui.on_search_changed(move |query| {
        let db = search_db.clone();
        let ui = search_ui.clone();
        let td = search_td.clone();
        let fd = search_fd.clone();
        // Match highlighting in ClipItem only applies while a query is active.
        let current_filter = search_ui
            .upgrade()
            .map(|u| {
                u.set_is_searching(!query.is_empty());
                u.get_active_filter().to_string()
            })
            .unwrap_or_default();
        let pfx = tag_prefix.clone();
        tokio::spawn(async move {
            crate::helpers::refresh_clips(
                &db,
                &ui,
                &td,
                &fd,
                query.as_str(),
                &current_filter,
                Some(&pfx),
            )
            .await;
        });
    });

    let backspace_ui = ui.as_weak();
    ui.on_search_backspace(move || {
        let ui = backspace_ui.clone();
        let _ = ui.upgrade_in_event_loop(move |ui| {
            let text = ui.get_search_text();
            if !text.is_empty() {
                let mut graphemes =
                    unicode_segmentation::UnicodeSegmentation::graphemes(text.as_str(), true);
                graphemes.next_back();
                let new_text: String = graphemes.collect();
                ui.set_search_text(new_text.clone().into());
                ui.invoke_search_changed(new_text.into());
            }
        });
    });

    // Ctrl+Backspace from the app-level FocusScope (when the LineEdit isn't
    // focused). Deletes the trailing word, mirroring Slint's own LineEdit
    // word-boundary logic (prev_word_boundary via unicode_word_indices).
    let word_backspace_ui = ui.as_weak();
    ui.on_search_word_backspace(move || {
        let ui = word_backspace_ui.clone();
        let _ = ui.upgrade_in_event_loop(move |ui| {
            let text = ui.get_search_text();
            if !text.is_empty() {
                let mut word_offset = 0;
                for (offset, _) in
                    unicode_segmentation::UnicodeSegmentation::unicode_word_indices(text.as_str())
                {
                    if offset <= text.len() {
                        word_offset = offset;
                    } else {
                        break;
                    }
                }
                let new_text: String = text[..word_offset].to_string();
                ui.set_search_text(new_text.clone().into());
                ui.invoke_search_changed(new_text.into());
            }
        });
    });
}

pub fn setup_filter(
    ui: &crate::AppWindow,
    db: &Arc<cliptoo_core::db::DbPool>,
    dirs: &crate::app_dirs::AppDirs,
    active_filter_state: &Arc<std::sync::Mutex<String>>,
) {
    let filter_db = db.clone();
    let filter_ui = ui.as_weak();
    let filter_td = dirs.thumbnails_dir.clone();
    let filter_fd = dirs.favicons_dir.clone();
    let filter_state = active_filter_state.clone();
    ui.on_filter_changed(move |filter| {
        // Keep the mirror in sync so background tasks (the clipboard listener)
        // can refresh with the active filter, even though Slint properties are
        // only readable on the UI thread.
        *filter_state.lock().unwrap() = filter.to_string();
        let db = filter_db.clone();
        let ui = filter_ui.clone();
        let td = filter_td.clone();
        let fd = filter_fd.clone();
        // A filter change clears the search text, so FTS highlighting is off.
        if let Some(u) = filter_ui.upgrade() {
            u.set_is_searching(false);
            u.set_search_text("".into());
        }
        let f = filter.to_string();
        tokio::spawn(async move {
            crate::helpers::refresh_clips(&db, &ui, &td, &fd, "", &f, None).await;
        });
    });
}

#[cfg(test)]
mod tests {
    use super::parse_match_spans;
    use slint::Model;

    fn spans_to_strings(model: &slint::ModelRc<crate::MatchSpan>) -> Vec<(String, bool)> {
        let mut out = Vec::new();
        for i in 0..model.row_count() {
            let s = model.row_data(i).unwrap();
            out.push((s.text.to_string(), s.is_highlight));
        }
        out
    }

    #[test]
    fn parse_highlights_only_matches_not_ellipsis() {
        // A multi-line clip produces a long snippet with FTS5 ellipsis markers
        // at the elided edges. The markers must stay outside highlighted spans.
        let snippet =
            "…[HL]the[/HL] quick brown fox jumps\nover the lazy dog\n…[HL]These[/HL] are separate…";
        let spans = spans_to_strings(&parse_match_spans(snippet));

        for (text, is_highlight) in &spans {
            if *is_highlight {
                assert!(
                    !text.contains('…'),
                    "highlighted span contains the FTS ellipsis: {text:?}"
                );
            }
        }
        assert_eq!(spans.len(), 5, "unexpected span layout: {spans:?}");
    }

    #[test]
    fn parse_single_line_snippet() {
        let spans = spans_to_strings(&parse_match_spans("foo [HL]bar[/HL] baz"));
        assert_eq!(
            spans,
            vec![
                ("foo ".to_string(), false),
                ("bar".to_string(), true),
                (" baz".to_string(), false),
            ]
        );
    }
}
