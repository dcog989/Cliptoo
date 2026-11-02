use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use slint::{ComponentHandle, Model};

/// Rows fetched per list page. Kept small so the first paint only decodes the
/// thumbnails it shows; `load_more` appends the next page on scroll.
const CLIP_PAGE_SIZE: i64 = 200;

/// User agent sent with every page and favicon request.
pub const USER_AGENT: &str = "Cliptoo/0.2";

/// Largest page or icon body accepted for a link preview. A page can be
/// arbitrarily large, and any app can put a URL on the clipboard, so the body
/// is capped instead of being read into memory in full.
pub const MAX_FETCH_BYTES: usize = 2 * 1024 * 1024;

/// Read a response body up to [`MAX_FETCH_BYTES`]. `None` when the response
/// declares or streams more than that.
pub async fn read_body_capped(resp: reqwest::Response) -> Option<Vec<u8>> {
    if resp
        .content_length()
        .is_some_and(|len| len > MAX_FETCH_BYTES as u64)
    {
        return None;
    }
    let mut out = Vec::new();
    let mut resp = resp;
    loop {
        match resp.chunk().await {
            Ok(Some(chunk)) => {
                if out.len() + chunk.len() > MAX_FETCH_BYTES {
                    return None;
                }
                out.extend_from_slice(&chunk);
            }
            Ok(None) => return Some(out),
            Err(_) => return None,
        }
    }
}

/// Whether a link-preview fetch may target this URL. A copied link can point
/// at the user's own machine or LAN (a pasted URL, or a page that puts one on
/// the clipboard); the preview must not probe those. Callers must apply this
/// to redirect targets too.
///
/// shortcut: only the literal host is checked, so a public name that resolves
/// to a private address still gets fetched; pin the resolved IP if this ever
/// runs somewhere other than the user's own desktop.
pub fn is_fetchable_url(url: &str) -> bool {
    let Ok(parsed) = url::Url::parse(url) else {
        return false;
    };
    match parsed.host() {
        Some(url::Host::Domain(domain)) => {
            let domain = domain.trim_end_matches('.');
            domain != "localhost" && !domain.ends_with(".localhost")
        }
        Some(url::Host::Ipv4(ip)) => {
            !(ip.is_private() || ip.is_loopback() || ip.is_link_local() || ip.is_unspecified())
        }
        Some(url::Host::Ipv6(ip)) => {
            // `::ffff:127.0.0.1` is loopback on the wire but not per flag, and
            // `::` is "any address", which Linux connects to localhost.
            let mapped = ip.to_ipv4_mapped().is_some_and(|v4| {
                v4.is_private() || v4.is_loopback() || v4.is_link_local() || v4.is_unspecified()
            });
            !(ip.is_loopback()
                || ip.is_unique_local()
                || ip.is_unicast_link_local()
                || ip.is_unspecified()
                || mapped)
        }
        None => false,
    }
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

/// Re-query using the view (query/filter) the window shows right now. The live
/// Slint properties are read in an event-loop callback, so a background ingest
/// can never apply results for a view the user has already left.
pub fn refresh_clips_live(
    db: &Arc<cliptoo_core::db::DbPool>,
    ui: &slint::Weak<crate::AppWindow>,
    td: &Path,
    fd: &Path,
    tag_prefix: Option<&str>,
) {
    let db = db.clone();
    let td = td.to_path_buf();
    let fd = fd.to_path_buf();
    let prefix = tag_prefix.map(str::to_string);
    let weak = ui.clone();
    let _ = weak.upgrade_in_event_loop(move |ui| {
        let query = ui.get_search_text().to_string();
        let filter = ui.get_active_filter().to_string();
        let ui = ui.as_weak();
        tokio::spawn(async move {
            refresh_clips(&db, &ui, &td, &fd, &query, &filter, prefix.as_deref()).await;
        });
    });
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
                CLIP_PAGE_SIZE,
                0,
                tag_prefix,
            )
        })
        .await;
    if let Err(e) = &result {
        // A failed query must not look like "the search box ignores me": the
        // model below is left untouched, so say so instead of silently
        // keeping the previous list.
        tracing::error!(
            "refresh_clips: query {:?}/{:?} failed: {e:#}",
            query,
            filter
        );
        let _ = ui.upgrade_in_event_loop(|ui| {
            ui.set_toast_message("Search failed; see the log".into());
            ui.set_toast_severity("error".into());
            ui.set_toast_visible(true);
        });
    }
    if let Ok(clips) = result {
        // A full page means older rows exist beyond it; the list appends the
        // next one as it nears the end (see `load_more_clips_live`).
        let has_more = clips.len() as i64 >= CLIP_PAGE_SIZE;
        let db2 = db.clone();
        let td = td.to_path_buf();
        let fd = fd.to_path_buf();
        // Snapshot the view so a result built for a query/filter the user has
        // since left is dropped instead of overwriting the newer list.
        let query = query.to_string();
        let filter = filter.to_string();
        let _ = ui.upgrade_in_event_loop(move |ui| {
            if ui.get_search_text().as_str() != query || ui.get_active_filter().as_str() != filter {
                return;
            }
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
            ui.set_clips_has_more(has_more);
            let new_idx = prev_selected.and_then(|id| {
                (0..model.row_count()).find(|&i| model.row_data(i).is_some_and(|d| d.id == id))
            });
            ui.set_selected_index(new_idx.map(|i| i as i32).unwrap_or(0));
            crate::favicon::check_pending_favicons(&ui, &db2, &fd);
        });
    }
}

/// Query one page past the rows the model already holds and append it.
///
/// Called from the list's `load-more` callback (UI thread), so the current
/// view, the offset and the model are all read before the query runs. The
/// `pending` flag holds the request to one page in flight: a wheel fling fires
/// the list's near-end check on several ticks, and two queries at the same
/// offset would append the same rows twice. It is cleared once the page is
/// applied, or on any path that drops it.
pub fn load_more_clips_live(
    db: &Arc<cliptoo_core::db::DbPool>,
    ui: &slint::Weak<crate::AppWindow>,
    td: &Path,
    fd: &Path,
    tag_prefix: Option<&str>,
    pending: Arc<AtomicBool>,
) {
    let Some(window) = ui.upgrade() else {
        pending.store(false, Ordering::Relaxed);
        return;
    };
    let offset = window.get_clips().row_count() as i64;
    let query = window.get_search_text().to_string();
    let filter = window.get_active_filter().to_string();
    drop(window);

    let db = db.clone();
    let ui = ui.clone();
    let td = td.to_path_buf();
    let fd = fd.to_path_buf();
    let prefix = tag_prefix.map(str::to_string);
    tokio::spawn(async move {
        let result = db
            .with(|conn| {
                cliptoo_core::db::queries::search_clips(
                    conn,
                    &query,
                    &filter,
                    CLIP_PAGE_SIZE,
                    offset,
                    prefix.as_deref(),
                )
            })
            .await;
        let clips = match result {
            Ok(clips) => clips,
            Err(e) => {
                tracing::error!("load_more: query {query:?}/{filter:?} at {offset} failed: {e:#}");
                pending.store(false, Ordering::Relaxed);
                return;
            }
        };
        // A short page ends the view: the list stops asking for more.
        let has_more = clips.len() as i64 >= CLIP_PAGE_SIZE;
        let done = pending.clone();
        let sent = ui.upgrade_in_event_loop(move |ui| {
            done.store(false, Ordering::Relaxed);
            // Drop a page built for a view the user has since left; mixing it
            // into the new model would interleave two result sets.
            if ui.get_search_text().as_str() != query || ui.get_active_filter().as_str() != filter {
                return;
            }
            ui.set_clips_has_more(has_more);
            let model = ui.get_clips();
            for clip in crate::thumbnail_cache::convert_vec(clips, &td, &fd) {
                if let Err(e) = model.insert_row(model.row_count(), clip) {
                    tracing::warn!("load_more: append failed: {e}");
                    break;
                }
            }
        });
        if sent.is_err() {
            pending.store(false, Ordering::Relaxed);
        }
    });
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
/// text query, no type/bookmark filter, and not truncated by paging. Only then
/// does the model's row order match the DB's `ORDER BY Timestamp DESC, Id DESC`,
/// so a single-row mutation can be mirrored in the model instead of re-querying
/// and rebuilding it (`search_clips` orders filtered or full-text results
/// differently, so those must re-query).
pub fn model_is_timestamp_ordered(ui: &crate::AppWindow) -> bool {
    ui.get_search_text().is_empty()
        && ui.get_active_filter().as_str() == "all"
        && model_is_complete(ui)
}

/// Whether the model holds every row of the current view, so removing one row in
/// place cannot leave a truncated list that a re-query would refill.
pub fn model_is_complete(ui: &crate::AppWindow) -> bool {
    !ui.get_clips_has_more()
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
    use super::is_fetchable_url;

    #[test]
    fn accepts_public_hosts() {
        for url in [
            "https://github.com/foo",
            "http://example.com",
            "https://[2001:db8::1]/x",
        ] {
            assert!(is_fetchable_url(url), "for {url}");
        }
    }

    #[test]
    fn refuses_local_and_private_hosts() {
        for url in [
            "http://localhost:8080/admin",
            "http://app.localhost/",
            "http://127.0.0.1/",
            "http://10.1.2.3/",
            "http://172.16.0.1/",
            "http://192.168.1.1/",
            "http://169.254.169.254/latest/meta-data/",
            "http://[::1]/",
            "http://[fd00::1]/",
            // Addresses that Linux routes to localhost even though the IP
            // predicates do not flag them as loopback.
            "http://0.0.0.0:8080/",
            "http://[::]/",
            "http://[::ffff:127.0.0.1]/",
            "http://localhost.:8080/admin",
            "not a url",
        ] {
            assert!(!is_fetchable_url(url), "for {url}");
        }
    }
}
