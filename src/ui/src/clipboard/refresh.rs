use std::path::Path;
use std::sync::Arc;

use cliptoo_core::db::DbPool;
use cliptoo_core::db::queries::{SEARCH_RESULT_LIMIT, search_clips};

pub(super) async fn refresh_ui(
    db: &Arc<DbPool>,
    ui: &slint::Weak<crate::AppWindow>,
    thumbnails_dir: &Path,
    favicons_dir: &Path,
) {
    let clips = db
        .with(|conn| search_clips(conn, "", "all", SEARCH_RESULT_LIMIT, 0, None))
        .await;
    if let Ok(clips) = clips {
        let dir = thumbnails_dir.to_owned();
        let fdir = favicons_dir.to_owned();
        let db_clone = db.clone();
        let _ = ui.upgrade_in_event_loop(move |ui| {
            use slint::ModelRc;
            let slint_clips = crate::thumbnail_cache::convert_vec(clips, &dir, &fdir);
            ui.set_clips(ModelRc::from(slint_clips.as_slice()));
            crate::favicon::check_pending_favicons(&ui, &db_clone, &fdir);
        });
    }
}
