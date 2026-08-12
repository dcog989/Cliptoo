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
