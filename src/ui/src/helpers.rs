use std::path::Path;
use std::sync::Arc;

use cliptoo_core::db::queries::SEARCH_RESULT_LIMIT;

const PAGE_TITLE_FETCH_TIMEOUT_SECS: u64 = 5;

pub const USER_AGENT: &str = "Cliptoo/0.2";

/// Show a toast notification on the main window.
/// `severity` is "info", "warn", or "error".
pub fn show_toast(ui: &slint::Weak<crate::AppWindow>, message: &str, severity: &str) {
    let msg = message.to_string();
    let sev = severity.to_string();
    let _ = ui.upgrade_in_event_loop(move |ui| {
        ui.set_toast_message(msg.into());
        ui.set_toast_severity(sev.into());
        ui.set_toast_visible(true);
    });
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
            let slint_clips = crate::thumbnail_cache::THUMB_LRU.with(|lru| {
                crate::thumbnail_cache::convert_vec_cached(clips, &td, &fd, &mut lru.borrow_mut())
            });
            let model = std::rc::Rc::new(slint::VecModel::<crate::ClipData>::from(slint_clips));
            ui.set_clips(model.into());
            ui.set_selected_index(0);
            crate::favicon::check_pending_favicons(&ui, &db2, &fd);
        });
    }
}
