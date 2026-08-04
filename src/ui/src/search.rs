use std::sync::Arc;

use slint::ComponentHandle;

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
