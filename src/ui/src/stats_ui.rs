// stats_ui.rs — Refreshes the Settings > Data statistics section.
// Queries the database (async, off the UI thread) each time the Data tab is
// shown and pushes the values into the SettingsWindow properties.

use chrono::{DateTime, Local, NaiveDateTime, Utc};
use cliptoo_core::db::queries;
use cliptoo_core::stats;
use slint::ComponentHandle;
use std::path::Path;
use std::sync::Arc;

/// Sum of the main DB file plus its WAL/SHM sidecars, which hold committed
/// data in WAL mode until checkpoint.
fn db_size_on_disk(db_path: &Path) -> u64 {
    let mut total = std::fs::metadata(db_path).map(|m| m.len()).unwrap_or(0);
    for suffix in ["-wal", "-shm"] {
        let mut p = db_path.as_os_str().to_owned();
        p.push(suffix);
        total += std::fs::metadata(Path::new(&p))
            .map(|m| m.len())
            .unwrap_or(0);
    }
    total
}

fn format_size(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = 1024 * KB;
    const GB: u64 = 1024 * MB;
    if bytes >= GB {
        format!("{:.1} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.1} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.1} KB", bytes as f64 / KB as f64)
    } else {
        format!("{bytes} B")
    }
}

/// The creation timestamp is stored as SQLite `datetime('now')` — UTC. Show it
/// in the local timezone.
fn format_created(raw: &str) -> String {
    match NaiveDateTime::parse_from_str(raw, "%Y-%m-%d %H:%M:%S") {
        Ok(naive) => DateTime::<Utc>::from_naive_utc_and_offset(naive, Utc)
            .with_timezone(&Local)
            .format("%Y-%m-%d %H:%M")
            .to_string(),
        Err(_) => raw.to_string(),
    }
}

pub fn setup_stats(
    settings_win: &crate::SettingsWindow,
    db: &Arc<cliptoo_core::db::DbPool>,
    db_path: &Path,
) {
    let win = settings_win.as_weak();
    let db = db.clone();
    let db_path = db_path.to_path_buf();

    settings_win.on_data_tab_shown(move || {
        let db = db.clone();
        let db_path = db_path.clone();
        let win = win.clone();
        tokio::spawn(async move {
            let stats_result: anyhow::Result<Stats> = db
                .with(|conn| {
                    Ok(Stats {
                        total: queries::count_clips(conn)?,
                        created: stats::get_stat(conn, stats::KEY_CREATION)?,
                        copies: stats::get_stat(conn, stats::KEY_UNIQUE_CLIPS_EVER)?,
                        pastes: stats::get_stat(conn, stats::KEY_PASTE_COUNT)?,
                    })
                })
                .await;
            let size = db_size_on_disk(&db_path);

            let _ = win.upgrade_in_event_loop(move |win| match stats_result {
                Ok(s) => {
                    win.set_s_total_clips(s.total.to_string().into());
                    win.set_s_db_created(
                        s.created
                            .as_deref()
                            .map(format_created)
                            .unwrap_or_default()
                            .into(),
                    );
                    win.set_s_db_size(format_size(size).into());
                    win.set_s_copies(
                        s.copies
                            .as_deref()
                            .and_then(|v| v.parse().ok())
                            .unwrap_or(0)
                            .to_string()
                            .into(),
                    );
                    win.set_s_pastes(
                        s.pastes
                            .as_deref()
                            .and_then(|v| v.parse().ok())
                            .unwrap_or(0)
                            .to_string()
                            .into(),
                    );
                }
                Err(e) => tracing::warn!("stats: failed to read database stats: {e}"),
            });
        });
    });
}

struct Stats {
    total: i64,
    created: Option<String>,
    copies: Option<String>,
    pastes: Option<String>,
}
