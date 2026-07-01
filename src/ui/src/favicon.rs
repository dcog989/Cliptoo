use std::path::{Path, PathBuf};
use std::sync::Arc;

use cliptoo_core::db::DbPool;
use slint::ComponentHandle;
use slint::Model;

const FAVICON_FETCH_TIMEOUT_SECS: u64 = 3;

fn extract_domain(url: &str) -> Option<String> {
    let stripped = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))?;
    Some(stripped.split('/').next()?.to_string())
}

pub async fn fetch_favicon(url: &str, fav_dir: &Path) -> Option<PathBuf> {
    let domain = extract_domain(url)?;
    let fav_path = fav_dir.join(format!("{domain}.webp"));
    if fav_path.exists() {
        let bytes = std::fs::read(&fav_path).ok()?;
        if image::load_from_memory(&bytes).is_ok() {
            return Some(fav_path);
        }
        let _ = std::fs::remove_file(&fav_path);
    }
    let fallback_url = format!("https://icons.duckduckgo.com/ip3/{domain}.ico");
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(FAVICON_FETCH_TIMEOUT_SECS))
        .user_agent(crate::helpers::USER_AGENT)
        .build()
        .ok()?;
    if let Ok(resp) = client.get(&fallback_url).send().await
        && resp.status().is_success()
    {
        let bytes = resp.bytes().await.ok()?;
        std::fs::create_dir_all(fav_dir).ok()?;
        if let Ok(img) = image::load_from_memory(&bytes)
            && img.save(&fav_path).is_ok()
        {
            return Some(fav_path);
        }
    }
    None
}

pub fn load_cached_page_title(url: &str, fav_dir: &Path) -> Option<String> {
    let domain = extract_domain(url)?;
    let path = fav_dir.join(format!("{domain}.title"));
    std::fs::read_to_string(&path).ok()
}

pub fn cache_page_title(url: &str, title: &str, fav_dir: &Path) {
    let domain = match extract_domain(url) {
        Some(d) => d,
        None => return,
    };
    let path = fav_dir.join(format!("{domain}.title"));
    let _ = std::fs::write(&path, title);
}

/// After populating the clip list, scan for link clips without cached
/// favicons and fetch them in the background.  Updates the model row
/// in-place as each favicon arrives.
///
/// Must be called on the UI thread (e.g. inside `upgrade_in_event_loop`)
/// because it accesses the Slint model synchronously to collect pending
/// clip IDs.  The actual HTTP fetching happens on the tokio runtime.
pub fn check_pending_favicons(ui: &crate::AppWindow, db: &Arc<DbPool>, favicons_dir: &Path) {
    let model = ui.get_clips();
    let mut pending = Vec::new();
    for i in 0..model.row_count() {
        if let Some(data) = model.row_data(i) {
            let ct = data.clip_type.as_str();
            if (ct == "link" || ct == "file_link") && data.favicon_image.size().width == 0 {
                pending.push((i, data.id as i64));
            }
        }
    }
    if pending.is_empty() {
        return;
    }
    let weak = ui.as_weak();
    let db = db.clone();
    let fav_dir = favicons_dir.to_owned();
    for (row, clip_id) in pending {
        let weak = weak.clone();
        let db = db.clone();
        let fav_dir = fav_dir.clone();
        tokio::spawn(async move {
            if let Ok(content) = db
                .with(|conn| cliptoo_core::db::queries::get_clip_content(conn, clip_id))
                .await
                && let Some(fav_path) = fetch_favicon(&content, &fav_dir).await
            {
                let _ = weak.upgrade_in_event_loop(move |ui| {
                    let img = slint::Image::load_from_path(&fav_path).unwrap_or_default();
                    if img.size().width == 0 {
                        return;
                    }
                    let model = ui.get_clips();
                    if let Some(mut data) = model.row_data(row) {
                        data.favicon_image = img;
                        model.set_row_data(row, data);
                    }
                });
            }
        });
    }
}
