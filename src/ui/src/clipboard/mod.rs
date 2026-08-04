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
    blacklisted_apps: Vec<String>,
    preview_max_dim: u32,
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
            blacklisted_apps,
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
    },
    FileUri {
        hash: String,
        content: String,
        sup_hash: u64,
    },
}

fn is_blacklisted(source_app: Option<&str>, blacklist: &[String]) -> bool {
    source_app.is_some_and(|app| blacklist.iter().any(|b| app == b || app.ends_with(b)))
}
