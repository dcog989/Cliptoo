use std::path::Path;
use std::sync::Arc;

use cliptoo_core::db::queries::SEARCH_RESULT_LIMIT;
use slint::Model;

const PAGE_TITLE_FETCH_TIMEOUT_SECS: u64 = 5;

pub const USER_AGENT: &str = "Cliptoo/0.2";

/// Extract the domain from a URL (e.g. "https://github.com/foo" -> "github.com").
/// Returns the host component of an absolute URL; `None` when the input is not
/// a valid absolute URL with a host (e.g. a relative or scheme-less string).
pub fn extract_domain(url: &str) -> Option<String> {
    url::Url::parse(url).ok()?.host_str().map(ToOwned::to_owned)
}

pub async fn fetch_page_title(url: &str) -> Option<String> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(
            PAGE_TITLE_FETCH_TIMEOUT_SECS,
        ))
        .user_agent(USER_AGENT)
        .build()
        .ok()?;
    let resp = client.get(url).send().await.ok()?;
    let body = resp.text().await.ok()?;
    let re = regex::Regex::new(r"(?i)<title>([^<]+)</title>").ok()?;
    re.captures(&body)
        .and_then(|c| c.get(1))
        .map(|m| m.as_str().trim().to_string())
}

/// Snapshot the list's active search query and filter on the UI thread, so a
/// background refresh triggered by a mutation (delete, move, paste bump, edit
/// save) keeps the user's current view instead of resetting to "all" while the
/// toolbar's filter icon and search box still show the old state. Call from a
/// UI-thread callback (Slint handlers), then pass the pair to `refresh_clips`.
pub fn current_view_state(ui: &slint::Weak<crate::AppWindow>) -> (String, String) {
    let Some(ui) = ui.upgrade() else {
        return (String::new(), String::new());
    };
    (
        ui.get_search_text().to_string(),
        ui.get_active_filter().to_string(),
    )
}

/// Query the DB for clips and replace the UI model.
pub async fn refresh_clips(
    db: &Arc<cliptoo_core::db::DbPool>,
    ui: &slint::Weak<crate::AppWindow>,
    td: &Path,
    fd: &Path,
    query: &str,
    filter: &str,
    tag_prefix: Option<&str>,
) {
    let result = db
        .with(|conn| {
            cliptoo_core::db::queries::search_clips(
                conn,
                query,
                filter,
                SEARCH_RESULT_LIMIT,
                0,
                tag_prefix,
            )
        })
        .await;
    if let Ok(clips) = result {
        let db2 = db.clone();
        let td = td.to_path_buf();
        let fd = fd.to_path_buf();
        let _ = ui.upgrade_in_event_loop(move |ui| {
            // Capture the currently selected clip so a background refresh
            // (e.g. a new clipboard ingest) doesn't yank the user's selection
            // and scroll position. If the clip is still in the new model it
            // stays selected; otherwise the selection falls back to the top.
            let prev_selected = {
                let idx = ui.get_selected_index();
                if idx >= 0 {
                    ui.get_clips().row_data(idx as usize).map(|d| d.id)
                } else {
                    None
                }
            };
            let slint_clips = crate::thumbnail_cache::convert_vec(clips, &td, &fd);
            let model = std::rc::Rc::new(slint::VecModel::<crate::ClipData>::from(slint_clips));
            ui.set_clips(model.clone().into());
            let new_idx = prev_selected.and_then(|id| {
                (0..model.row_count()).find(|&i| model.row_data(i).is_some_and(|d| d.id == id))
            });
            ui.set_selected_index(new_idx.map(|i| i as i32).unwrap_or(0));
            crate::favicon::check_pending_favicons(&ui, &db2, &fd);
        });
    }
}

/// A single-row reorder already applied to the DB, to mirror in the UI model.
#[derive(Clone, Copy)]
pub enum RowMove {
    Top,
    Bottom,
    Up,
    Down,
}

/// Whether the visible model is the full, timestamp-ordered clip list: no active
/// text query, no type/bookmark filter, and not truncated by
/// [`SEARCH_RESULT_LIMIT`]. Only then does the model's row order match the DB's
/// `ORDER BY Timestamp DESC, Id DESC`, so a single-row mutation can be mirrored
/// in the model instead of re-querying and rebuilding it (`search_clips` orders
/// filtered or full-text results differently, so those must re-query).
pub fn model_is_timestamp_ordered(ui: &crate::AppWindow) -> bool {
    ui.get_search_text().is_empty()
        && ui.get_active_filter().as_str() == "all"
        && model_is_complete(ui)
}

/// Whether the model holds every row of the current view, so removing one row in
/// place cannot leave a truncated list that a re-query would refill.
pub fn model_is_complete(ui: &crate::AppWindow) -> bool {
    ui.get_clips().row_count() < SEARCH_RESULT_LIMIT as usize
}

/// Mirror a clip reorder in the UI model in place, keeping the selected clip
/// selected. Does nothing unless [`model_is_timestamp_ordered`] still holds
/// when the mutation runs, so a view change since the DB write can't corrupt a
/// filtered or full-text-ordered model.
pub fn apply_row_move(ui: &crate::AppWindow, id: i32, how: RowMove) {
    if !model_is_timestamp_ordered(ui) {
        return;
    }
    let model = ui.get_clips();
    let Some(index) = row_index_of(&model, id) else {
        return;
    };
    let previous_selection = selected_clip_id(ui);
    let last = model.row_count().saturating_sub(1);
    match how {
        RowMove::Top if index > 0 => {
            if let Some(data) = model.row_data(index) {
                if let Err(e) = model.remove_row(index) {
                    tracing::warn!("apply_row_move(Top): remove row {index} failed: {e}");
                    return;
                }
                if let Err(e) = model.insert_row(0, data) {
                    tracing::warn!("apply_row_move(Top): insert row failed: {e}");
                    return;
                }
            }
        }
        RowMove::Bottom if index < last => {
            if let Some(data) = model.row_data(index) {
                if let Err(e) = model.remove_row(index) {
                    tracing::warn!("apply_row_move(Bottom): remove row {index} failed: {e}");
                    return;
                }
                if let Err(e) = model.push_row(data) {
                    tracing::warn!("apply_row_move(Bottom): push row failed: {e}");
                    return;
                }
            }
        }
        RowMove::Up if index > 0 => swap_rows(&model, index - 1, index),
        RowMove::Down if index < last => swap_rows(&model, index, index + 1),
        _ => {}
    }
    restore_selection(ui, previous_selection);
}

/// Remove a clip row from the UI model in place, keeping the selected clip
/// selected. Does nothing unless [`model_is_complete`] still holds when the
/// mutation runs.
pub fn remove_row_by_id(ui: &crate::AppWindow, id: i32) {
    if !model_is_complete(ui) {
        return;
    }
    let model = ui.get_clips();
    let Some(index) = row_index_of(&model, id) else {
        return;
    };
    let previous_selection = selected_clip_id(ui);
    if let Err(e) = model.remove_row(index) {
        tracing::warn!("remove_row_by_id: remove row {index} failed: {e}");
        return;
    }
    restore_selection(ui, previous_selection);
}

fn row_index_of(model: &slint::ModelRc<crate::ClipData>, id: i32) -> Option<usize> {
    (0..model.row_count()).find(|&i| model.row_data(i).is_some_and(|d| d.id == id))
}

fn selected_clip_id(ui: &crate::AppWindow) -> Option<i32> {
    let index = ui.get_selected_index();
    if index < 0 {
        return None;
    }
    ui.get_clips().row_data(index as usize).map(|d| d.id)
}

/// Re-point `selected-index` at `previous`'s new row, or fall back to the top
/// when it is gone. Mirrors the selection restore in [`refresh_clips`].
fn restore_selection(ui: &crate::AppWindow, previous: Option<i32>) {
    let model = ui.get_clips();
    let index = previous.and_then(|id| row_index_of(&model, id));
    ui.set_selected_index(index.map(|i| i as i32).unwrap_or(0));
}

fn swap_rows(model: &slint::ModelRc<crate::ClipData>, a: usize, b: usize) {
    if let (Some(a_data), Some(b_data)) = (model.row_data(a), model.row_data(b)) {
        model.set_row_data(a, b_data);
        model.set_row_data(b, a_data);
    }
}

#[cfg(test)]
mod tests {
    use super::extract_domain;

    #[test]
    fn extracts_host_from_http_urls() {
        assert_eq!(
            extract_domain("https://github.com/foo").unwrap(),
            "github.com"
        );
        assert_eq!(extract_domain("http://example.com").unwrap(), "example.com");
        assert_eq!(
            extract_domain("ftp://files.example.com/x").unwrap(),
            "files.example.com"
        );
    }

    #[test]
    fn normalizes_scheme_and_host_case() {
        assert_eq!(
            extract_domain("HTTP://EXAMPLE.COM/path").unwrap(),
            "example.com"
        );
    }

    #[test]
    fn strips_userinfo_port_and_handles_ipv6() {
        assert_eq!(
            extract_domain("https://user:pass@example.com").unwrap(),
            "example.com"
        );
        assert_eq!(
            extract_domain("https://example.com:8443/x").unwrap(),
            "example.com"
        );
        assert_eq!(extract_domain("https://[::1]:8080/x").unwrap(), "[::1]");
    }

    #[test]
    fn rejects_non_urls() {
        for s in [
            "",
            "not a url",
            "www.example.com",
            "example.com",
            "/relative/path",
        ] {
            assert_eq!(extract_domain(s), None, "for {s:?}");
        }
    }
}
