use std::path::PathBuf;
use std::sync::Arc;

use cliptoo_core::db::DbPool;

use crate::paste::PasteSuppressionSet;

mod listener;
mod reader;

pub use listener::run_listener;

/// Spawn the clipboard listener as a background task. Wraps `run_listener`
/// with error logging so the entrypoint doesn't inline the tokio spawn.
#[allow(clippy::too_many_arguments)]
pub fn spawn_listener(
    db: Arc<DbPool>,
    ui: slint::Weak<crate::AppWindow>,
    thumbnails_dir: PathBuf,
    favicons_dir: PathBuf,
    images_dir: PathBuf,
    suppression: Arc<PasteSuppressionSet>,
    blacklist_state: Arc<std::sync::Mutex<Vec<String>>>,
    preview_max_dim: Arc<std::sync::atomic::AtomicU32>,
    active_filter_state: Arc<std::sync::Mutex<String>>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        if let Err(e) = run_listener(
            db,
            ui,
            thumbnails_dir,
            favicons_dir,
            images_dir,
            suppression,
            blacklist_state,
            preview_max_dim,
            active_filter_state,
        )
        .await
        {
            tracing::error!("Clipboard listener error: {e}");
        }
    })
}

enum ClipboardPayload {
    Text {
        hash: String,
        content: String,
        sup_hash: u64,
    },
    Image {
        hash: String,
        data: Vec<u8>,
        sup_hash: u64,
        mime: String,
    },
    FileUri {
        content: String,
        sup_hash: u64,
    },
}

/// Test whether `source_app` (the active window's resource id) is excluded.
///
/// A blacklist entry matches the full app id or its resource name — the last
/// dot- or dash-separated component (e.g. "konsole" for "org.kde.konsole",
/// "chrome" for "google-chrome"). Matching is case-insensitive. A plain
/// `ends_with` would over-match unrelated ids that merely share a suffix.
fn is_blacklisted(source_app: Option<&str>, blacklist: &[String]) -> bool {
    source_app.is_some_and(|app| {
        let resource_name = app.rsplit(['.', '-']).next().unwrap_or(app);
        blacklist
            .iter()
            .any(|b| app.eq_ignore_ascii_case(b) || resource_name.eq_ignore_ascii_case(b))
    })
}
