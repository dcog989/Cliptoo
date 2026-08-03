use std::collections::HashMap;
use std::sync::Arc;

use futures::StreamExt;
use slint::ComponentHandle;
use zbus::{
    Connection, MessageStream,
    zvariant::{OwnedObjectPath, OwnedValue, Value},
};

use cliptoo_core::db::queries::SEARCH_RESULT_LIMIT;

const PORTAL_DEST: &str = "org.freedesktop.portal.Desktop";
const PORTAL_PATH: &str = "/org/freedesktop/portal/desktop";
const FILE_CHOOSER_IFACE: &str = "org.freedesktop.portal.FileChooser";
const REQUEST_IFACE: &str = "org.freedesktop.portal.Request";

/// Convert a `file://` URI to a plain filesystem path, or `None` if the URI
/// is not a file URI. Minimal percent-decoding for the path segment.
fn file_uri_to_path(uri: &str) -> Option<String> {
    let rest = uri.strip_prefix("file://")?;
    let mut out = String::with_capacity(rest.len());
    let bytes = rest.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            let hex = &rest[i + 1..i + 3];
            if let Ok(v) = u8::from_str_radix(hex, 16) {
                out.push(v as char);
                i += 3;
                continue;
            }
        }
        out.push(rest[i..].chars().next().unwrap_or('?'));
        i += rest[i..].chars().next().unwrap_or('?').len_utf8();
    }
    Some(out)
}

/// Ask the user for a file path via the XDG portal FileChooser.
/// `save = true` shows a Save dialog, `false` an Open dialog.
/// Returns `None` if the user cancelled.
async fn pick_path(save: bool) -> anyhow::Result<Option<std::path::PathBuf>> {
    let conn = Connection::session().await?;
    let mut stream = MessageStream::from(&conn);

    let method = if save { "SaveFile" } else { "OpenFile" };
    let title = if save {
        "Export Cliptoo history"
    } else {
        "Import Cliptoo history"
    };

    let mut options = HashMap::<&str, Value>::new();
    options.insert("handle_token", Value::from("cliptoo_maintenance"));
    options.insert("current_name", Value::from("cliptoo-export.json"));

    let request_handle: OwnedObjectPath = conn
        .call_method(
            Some(PORTAL_DEST),
            PORTAL_PATH,
            Some(FILE_CHOOSER_IFACE),
            method,
            &("", title, &options),
        )
        .await?
        .body()
        .deserialize()?;

    // Wait for the Response signal on the request handle.
    while let Some(Ok(msg)) = stream.next().await {
        let hdr = msg.header();
        let on_path = hdr.path().is_some_and(|p| p == request_handle.as_str());
        let on_iface = hdr.interface().is_some_and(|i| i.as_str() == REQUEST_IFACE);
        let is_response = hdr.member().is_some_and(|m| m.as_str() == "Response");
        if !(on_path && on_iface && is_response) {
            continue;
        }

        let (code, results): (u32, HashMap<String, OwnedValue>) = msg.body().deserialize()?;
        if code != 0 {
            return Ok(None);
        }

        let uris: Vec<String> = results
            .get("uris")
            .and_then(|v| v.clone().try_into().ok())
            .unwrap_or_default();
        let uri = uris.into_iter().next();
        return Ok(uri.and_then(|u| file_uri_to_path(&u).map(std::path::PathBuf::from)));
    }

    anyhow::bail!("file chooser message stream ended unexpectedly")
}

/// Show a toast on the settings window (the window the user is looking at).
fn toast(settings_win: &slint::Weak<crate::SettingsWindow>, message: &str, severity: &str) {
    let msg = message.to_string();
    let sev = severity.to_string();
    let _ = settings_win.upgrade_in_event_loop(move |win| {
        win.set_toast_message(msg.into());
        win.set_toast_severity(sev.into());
        win.set_toast_visible(true);
    });
}

/// Set up the manual maintenance actions handler on the main window.
pub fn setup_manual_maintenance(
    ui: &crate::AppWindow,
    db: &Arc<cliptoo_core::db::DbPool>,
    dirs: &crate::app_dirs::AppDirs,
    settings: &std::rc::Rc<std::cell::RefCell<cliptoo_core::Settings>>,
    settings_win: &crate::SettingsWindow,
) {
    let maint_db = db.clone();
    let maint_td = dirs.thumbnails_dir.clone();
    let maint_fd = dirs.favicons_dir.clone();
    let maint_settings = settings.clone();
    let maint_ui = ui.as_weak();
    let maint_win = settings_win.as_weak();
    let maint_td2 = dirs.thumbnails_dir.clone();
    ui.on_maintenance_action(move |key: slint::SharedString| {
        let db = maint_db.clone();
        let td = maint_td.clone();
        let fd = maint_fd.clone();
        let td2 = maint_td2.clone();
        let settings = maint_settings.clone();
        let ui = maint_ui.clone();
        let settings_win = maint_win.clone();
        let key = key.to_string();
        let (max_clips, max_age_days) = {
            let s = settings.borrow();
            (s.max_clips, s.max_age_days)
        };
        tokio::spawn(async move {
            let result: anyhow::Result<Option<String>> = async {
                match key.as_str() {
                    "clear-history" => {
                        db.with(|conn| {
                            cliptoo_core::maintenance::clear_history(conn, false).map(|_| ())
                        })
                        .await?;
                        Ok(None)
                    }
                    "clear-history-all" => {
                        db.with(|conn| {
                            cliptoo_core::maintenance::clear_history(conn, true).map(|_| ())
                        })
                        .await?;
                        Ok(None)
                    }
                    "clear-caches" => {
                        cliptoo_core::maintenance::prune_cache(&db, &td, &fd).await?;
                        Ok(None)
                    }
                    "deadhead" => {
                        let n = db
                            .with(|conn| cliptoo_core::maintenance::deadhead(conn))
                            .await?;
                        Ok(Some(if n == 0 {
                            "No dead file clips found".to_string()
                        } else {
                            format!("Removed {n} dead file clips")
                        }))
                    }
                    "reclassify" => {
                        let n = db
                            .with(|conn| cliptoo_core::maintenance::reclassify_all(conn))
                            .await?;
                        Ok(Some(if n == 0 {
                            "No clips reclassified".to_string()
                        } else {
                            format!("Reclassified {n} clips")
                        }))
                    }
                    "prune-oversized" => {
                        db.with(|conn| {
                            cliptoo_core::maintenance::prune_oversized(conn, 1_048_576).map(|_| ())
                        })
                        .await?;
                        Ok(None)
                    }
                    "export" => match pick_path(true).await? {
                        Some(path) => {
                            cliptoo_core::export::export_to_file(&db, &path).await?;
                            Ok(Some(format!("Exported to {}", path.display())))
                        }
                        None => Ok(None),
                    },
                    "export-bookmarks" => match pick_path(true).await? {
                        Some(path) => {
                            cliptoo_core::export::export_bookmarked_to_file(&db, &path).await?;
                            Ok(Some(format!("Exported bookmarks to {}", path.display())))
                        }
                        None => Ok(None),
                    },
                    "import" => match pick_path(false).await? {
                        Some(path) => {
                            let count = cliptoo_core::export::import_from_file(&db, &path).await?;
                            let msg = format!("Imported {count} clips from {}", path.display());
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
                            Ok(Some(msg))
                        }
                        None => Ok(None),
                    },
                    other => {
                        tracing::warn!("maintenance_action: unknown key '{other}'");
                        Ok(None)
                    }
                }
            }
            .await;

            // Cancelled file dialog: nothing to do, keep the list as-is.
            let cancelled =
                matches!(&result, Ok(None)) && matches!(key.as_str(), "export" | "import");

            match &result {
                Ok(Some(msg)) => toast(&settings_win, msg, "info"),
                Ok(None) if !cancelled => {
                    let msg = match key.as_str() {
                        "clear-history" => "History cleared",
                        "clear-history-all" => "Full history cleared",
                        "clear-caches" => "Caches pruned",
                        "prune-oversized" => "Oversized clips removed",
                        _ => "",
                    };
                    if !msg.is_empty() {
                        toast(&settings_win, msg, "info");
                    }
                }
                Ok(None) => {}
                Err(e) => {
                    tracing::error!("maintenance_action '{key}' failed: {e}");
                    toast(&settings_win, &format!("Error: {e}"), "error");
                }
            }

            if !cancelled {
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
                        let slint_clips =
                            crate::thumbnail_cache::convert_vec(clips, &td2, &fd_refresh);
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
            }

            Ok::<Option<String>, anyhow::Error>(None)
        });
    });
}
