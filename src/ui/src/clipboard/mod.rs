use std::path::PathBuf;
use std::sync::Arc;

use cliptoo_core::db::DbPool;
use tokio::sync::mpsc;

use crate::paste::PasteSuppressionSet;
use crate::source_app::ActiveWindowCache;

mod listener;
mod reader;
mod watcher;

pub use listener::run_listener;
pub use watcher::spawn_clipboard_watcher;

/// Buffer of pending selection-change notifications from the watcher thread to
/// the listener. Sized to absorb copy bursts (e.g. rapid re-copies) without a
/// source-app snapshot being dropped.
pub const SELECTION_CHANNEL_CAPACITY: usize = 16;

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
    selection_rx: mpsc::Receiver<Option<String>>,
    active_window: ActiveWindowCache,
    watcher_connected: Arc<std::sync::atomic::AtomicBool>,
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
            selection_rx,
            active_window,
            watcher_connected,
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

/// Test whether a clip from `source_app` (the active window's resource id)
/// must be excluded.
///
/// A blacklist entry matches the full app id or its resource name — the last
/// dot- or dash-separated component (e.g. "konsole" for "org.kde.konsole",
/// "chrome" for "google-chrome"). Matching is case-insensitive. A plain
/// `ends_with` would over-match unrelated ids that merely share a suffix.
///
/// Fails closed: when a blacklist is configured but the source app is unknown
/// (tracker down, window without a resource class), the clip is excluded.
/// Silently capturing a "blacklisted" app's copy is the worse failure.
fn is_blacklisted(source_app: Option<&str>, blacklist: &[String]) -> bool {
    if blacklist.is_empty() {
        return false;
    }
    let Some(app) = source_app else {
        return true;
    };
    let resource_name = app.rsplit(['.', '-']).next().unwrap_or(app);
    blacklist
        .iter()
        .any(|b| app.eq_ignore_ascii_case(b) || resource_name.eq_ignore_ascii_case(b))
}

#[cfg(test)]
mod tests {
    use super::is_blacklisted;

    fn list(entries: &[&str]) -> Vec<String> {
        entries.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn matches_the_full_id_and_its_resource_name() {
        let blacklist = list(&["org.kde.konsole", "chrome"]);
        assert!(is_blacklisted(Some("org.kde.konsole"), &blacklist));
        assert!(is_blacklisted(Some("google-chrome"), &blacklist));
        assert!(is_blacklisted(Some("CHROME"), &blacklist));
    }

    #[test]
    fn does_not_match_a_suffix_of_the_resource_name() {
        let blacklist = list(&["konsole", "chrome"]);
        assert!(!is_blacklisted(Some("org.kde.konsole-x"), &blacklist));
        assert!(!is_blacklisted(Some("org.example.chromebook"), &blacklist));
    }

    #[test]
    fn empty_blacklist_captures_unknown_sources() {
        assert!(!is_blacklisted(None, &[]));
    }

    #[test]
    fn configured_blacklist_excludes_an_unknown_source() {
        assert!(is_blacklisted(None, &list(&["konsole"])));
    }
}
