use std::sync::Arc;

use slint::ComponentHandle;

use cliptoo_core::db::queries::SEARCH_RESULT_LIMIT;

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
        let current_filter = search_ui
            .upgrade()
            .map(|u| u.get_active_filter().to_string())
            .unwrap_or_default();
        let pfx = tag_prefix.clone();
        tokio::spawn(async move {
            let result = db
                .with(|conn| {
                    cliptoo_core::db::queries::search_clips(
                        conn,
                        query.as_str(),
                        &current_filter,
                        SEARCH_RESULT_LIMIT,
                        0,
                        Some(pfx.as_str()),
                    )
                })
                .await;
            if let Ok(clips) = result {
                let db2 = db.clone();
                let _ = ui.upgrade_in_event_loop(move |ui| {
                    let slint_clips = crate::thumbnail_cache::THUMB_LRU.with(|lru| {
                        crate::thumbnail_cache::convert_vec_cached(
                            clips,
                            &td,
                            &fd,
                            &mut lru.borrow_mut(),
                        )
                    });
                    let model =
                        std::rc::Rc::new(slint::VecModel::<crate::ClipData>::from(slint_clips));
                    ui.set_clips(model.into());
                    ui.set_selected_index(0);
                    crate::favicon::check_pending_favicons(&ui, &db2, &fd);
                });
            }
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
}

pub fn setup_filter(
    ui: &crate::AppWindow,
    db: &Arc<cliptoo_core::db::DbPool>,
    dirs: &crate::app_dirs::AppDirs,
) {
    let filter_db = db.clone();
    let filter_ui = ui.as_weak();
    let filter_td = dirs.thumbnails_dir.clone();
    let filter_fd = dirs.favicons_dir.clone();
    ui.on_filter_changed(move |filter| {
        let db = filter_db.clone();
        let ui = filter_ui.clone();
        let td = filter_td.clone();
        let fd = filter_fd.clone();
        tokio::spawn(async move {
            let result = db
                .with(|conn| {
                    cliptoo_core::db::queries::search_clips(
                        conn,
                        "",
                        filter.as_str(),
                        SEARCH_RESULT_LIMIT,
                        0,
                        None,
                    )
                })
                .await;
            if let Ok(clips) = result {
                let db2 = db.clone();
                let _ = ui.upgrade_in_event_loop(move |ui| {
                    let slint_clips = crate::thumbnail_cache::THUMB_LRU.with(|lru| {
                        crate::thumbnail_cache::convert_vec_cached(
                            clips,
                            &td,
                            &fd,
                            &mut lru.borrow_mut(),
                        )
                    });
                    let model =
                        std::rc::Rc::new(slint::VecModel::<crate::ClipData>::from(slint_clips));
                    ui.set_clips(model.into());
                    ui.set_selected_index(0);
                    ui.set_search_text("".into());
                    crate::favicon::check_pending_favicons(&ui, &db2, &fd);
                });
            }
        });
    });
}
