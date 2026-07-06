use std::sync::Arc;

use slint::ComponentHandle;
use tracing::info;

use cliptoo_core::db::queries::SEARCH_RESULT_LIMIT;

use crate::helpers;

/// Set up the manual maintenance actions handler on the main window.
pub fn setup_manual_maintenance(
    ui: &crate::AppWindow,
    db: &Arc<cliptoo_core::db::DbPool>,
    dirs: &crate::app_dirs::AppDirs,
    settings: &std::rc::Rc<std::cell::RefCell<cliptoo_core::Settings>>,
) {
    let maint_db = db.clone();
    let maint_td = dirs.thumbnails_dir.clone();
    let maint_fd = dirs.favicons_dir.clone();
    let maint_data_dir = dirs.data_dir.clone();
    let maint_settings = settings.clone();
    let maint_ui = ui.as_weak();
    let maint_td2 = dirs.thumbnails_dir.clone();
    ui.on_maintenance_action(move |key: slint::SharedString| {
        let db = maint_db.clone();
        let td = maint_td.clone();
        let fd = maint_fd.clone();
        let data_dir = maint_data_dir.clone();
        let td2 = maint_td2.clone();
        let settings = maint_settings.clone();
        let ui = maint_ui.clone();
        let key = key.to_string();
        let (max_clips, max_age_days) = {
            let s = settings.borrow();
            (s.max_clips, s.max_age_days)
        };
        tokio::spawn(async move {
            let result: anyhow::Result<()> = match key.as_str() {
                "clear-history" => {
                    db.with(|conn| {
                        cliptoo_core::maintenance::clear_history(conn, false).map(|_| ())
                    })
                    .await
                }
                "clear-history-all" => {
                    db.with(|conn| cliptoo_core::maintenance::clear_history(conn, true).map(|_| ()))
                        .await
                }
                "clear-caches" => cliptoo_core::maintenance::prune_cache(&db, &td, &fd)
                    .await
                    .map(|_| ()),
                "deadhead" => {
                    db.with(|conn| cliptoo_core::maintenance::deadhead(conn).map(|_| ()))
                        .await
                }
                "reclassify" => {
                    db.with(|conn| cliptoo_core::maintenance::reclassify_all(conn).map(|_| ()))
                        .await
                }
                "prune-oversized" => {
                    db.with(|conn| {
                        cliptoo_core::maintenance::prune_oversized(conn, 1_048_576).map(|_| ())
                    })
                    .await
                }
                "export" => {
                    let ts = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .ok()
                        .map(|d| d.as_secs())
                        .unwrap_or(0);
                    let path = data_dir.join(format!("export-{ts}.json"));
                    cliptoo_core::export::export_to_file(&db, &path)
                        .await
                        .map(|_| ())
                }
                "import" => {
                    let path = data_dir.join("import.json");
                    let count = cliptoo_core::export::import_from_file(&db, &path).await;
                    match count {
                        Ok(n) => {
                            info!("imported {n} clips from {:?}", path);
                            let td_import = td2.clone();
                            let fd_import = fd.clone();
                            if let Ok(clips) = db
                                .with(|conn| {
                                    cliptoo_core::db::queries::search_clips(
                                        conn,
                                        "",
                                        "",
                                        SEARCH_RESULT_LIMIT,
                                        0,
                                        None,
                                    )
                                })
                                .await
                            {
                                let db_import = db.clone();
                                let _ = ui.upgrade_in_event_loop(move |ui| {
                                    let slint_clips = crate::thumbnail_cache::convert_vec(
                                        clips, &td_import, &fd_import,
                                    );
                                    ui.set_clips(slint::ModelRc::from(slint_clips.as_slice()));
                                    ui.set_selected_index(0);
                                    crate::favicon::check_pending_favicons(
                                        &ui, &db_import, &fd_import,
                                    );
                                });
                            }
                        }
                        Err(e) => {
                            tracing::error!("import failed: {e}");
                        }
                    }
                    Ok(())
                }
                other => {
                    tracing::warn!("maintenance_action: unknown key '{other}'");
                    Ok(())
                }
            };

            if let Err(e) = &result {
                tracing::error!("maintenance_action '{key}' failed: {e}");
                helpers::show_toast(&ui, &format!("Error: {e}"), "error");
            } else {
                let msg = match key.as_str() {
                    "clear-history" => "History cleared",
                    "clear-history-all" => "Full history cleared",
                    "clear-caches" => "Caches pruned",
                    "deadhead" => "Dead file clips removed",
                    "reclassify" => "Clips reclassified",
                    "prune-oversized" => "Oversized clips removed",
                    "export" => "Export complete",
                    "import" => "Import complete",
                    _ => "",
                };
                if !msg.is_empty() {
                    helpers::show_toast(&ui, msg, "info");
                }
            }

            let need_refresh = key != "import";
            if need_refresh
                && let Ok(clips) = db
                    .with(|conn| {
                        cliptoo_core::db::queries::search_clips(
                            conn,
                            "",
                            "",
                            SEARCH_RESULT_LIMIT,
                            0,
                            None,
                        )
                    })
                    .await
            {
                let fd_refresh = fd.clone();
                let db_refresh = db.clone();
                let _ = ui.upgrade_in_event_loop(move |ui| {
                    let slint_clips = crate::thumbnail_cache::convert_vec(clips, &td2, &fd_refresh);
                    ui.set_clips(slint::ModelRc::from(slint_clips.as_slice()));
                    ui.set_selected_index(0);
                    crate::favicon::check_pending_favicons(&ui, &db_refresh, &fd_refresh);
                });
            }

            match key.as_str() {
                "clear-history" | "clear-history-all" | "deadhead" | "prune-oversized"
                | "reclassify" => {
                    let _ = cliptoo_core::maintenance::run_scheduled(
                        &db,
                        cliptoo_core::maintenance::RetentionConfig {
                            max_clips,
                            max_age_days,
                        },
                        &td,
                        &fd,
                    )
                    .await;
                }
                _ => {}
            }
        });
    });
}
