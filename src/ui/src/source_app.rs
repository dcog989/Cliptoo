//! Continuously track the application that owns the active window (the
//! "source app" for a copy) and expose it as a shared, near-real-time cache.
//!
//! KWin 6 removed the `org.kde.KWin.activeWindow` D-Bus method and reworked
//! `getWindowInfo` to take a window `uuid`; the only supported way to learn
//! the active window's application is the KWin scripting interface
//! (`org.kde.KWin` at `/Scripting`). A resident script subscribes to
//! `workspace.windowActivated` and pushes the active window's `resourceClass`
//! back to us with `callDBus` on every focus change; the callbacks arrive on
//! the shared session connection via a `MessageStream` match rule (zbus
//! broadcasts every incoming message to all matching subscribers, so this
//! doesn't disturb the object server).
//!
//! A fresh on-demand query at ingest time is always stale: the listener
//! notices a copy up to one poll interval late, by which point focus may have
//! moved on. The clipboard watcher (`clipboard::watcher`) instead snapshots
//! this cache at the exact moment a selection change arrives, attributing the
//! copy to the app that actually produced it.

use std::sync::Arc;
use std::sync::RwLock;
use std::time::Duration;

use futures::StreamExt;
use tracing::debug;
use zbus::message::Type;
use zbus::{MatchRule, MessageStream};

const KW_SCRIPTING_SERVICE: &str = "org.kde.KWin";
const KW_SCRIPTING_PATH: &str = "/Scripting";
const KW_SCRIPTING_IFACE: &str = "org.kde.kwin.Scripting";
const KW_SCRIPT_IFACE: &str = "org.kde.kwin.Script";
const SCRIPT_TIMEOUT: Duration = Duration::from_secs(2);
// The D-Bus member the tracker script calls back on for every focus change.
const SCRIPT_MEMBER: &str = "activeWindow";
// Fixed (not pid-suffixed) so a fresh process unloads the resident script a
// crashed or killed previous instance left behind; otherwise one script would
// leak into KWin per restart, calling a dead bus name on every focus change.
const SCRIPT_NAME: &str = "cliptoo-activewindow";
// Re-install cadence when the script can't be loaded or the stream dies
// (e.g. KWin restarted underneath us). The cache keeps its last value during
// the outage, which is better than nothing.
const RETRY_DELAY: Duration = Duration::from_secs(30);

/// Shared cache of the active window's app id (`resourceClass`), kept current
/// by a resident KWin script. `None` while unknown (no KWin, scripting
/// unavailable, no active window).
pub type ActiveWindowCache = Arc<RwLock<Option<String>>>;

/// Start the background tracker and return the cache it maintains.
pub fn spawn_active_window_tracker() -> ActiveWindowCache {
    let cache: ActiveWindowCache = Arc::new(RwLock::new(None));
    let retry_cache = cache.clone();
    tokio::spawn(async move {
        let mut consecutive_failures: u32 = 0;
        loop {
            match install_tracker(&retry_cache).await {
                Ok(()) => {
                    debug!("active-window tracker stream ended; re-installing");
                    consecutive_failures = 0;
                }
                Err(e) => {
                    // Warn once, then go quiet: on a system without KWin the
                    // tracker can never load and must not spam the log.
                    if consecutive_failures == 0 {
                        tracing::warn!(
                            "active-window tracker unavailable: {e}; will keep retrying"
                        );
                    } else {
                        debug!("active-window tracker retry failed: {e}");
                    }
                    consecutive_failures = consecutive_failures.saturating_add(1);
                }
            }
            tokio::time::sleep(RETRY_DELAY).await;
        }
    });
    cache
}

/// Read the current active-window app id, if known.
pub fn current_active_app(cache: &ActiveWindowCache) -> Option<String> {
    cache.read().ok().and_then(|guard| guard.clone())
}

/// Load the resident KWin script, subscribe to its callbacks, and keep the
/// cache current until the stream ends.
async fn install_tracker(cache: &ActiveWindowCache) -> anyhow::Result<()> {
    let conn = crate::dbus::session().await?;
    let our_name = conn
        .unique_name()
        .ok_or_else(|| anyhow::anyhow!("no unique D-Bus name available"))?
        .to_string();

    // Subscribe before loading the script so a fast initial `report()` can't
    // race ahead of us. The callback is a method call to our unique name with
    // member "activeWindow".
    let rule = MatchRule::builder()
        .msg_type(Type::MethodCall)
        .member(SCRIPT_MEMBER)
        .ok()
        .ok_or_else(|| anyhow::anyhow!("failed to build match rule"))?
        .build();
    let mut stream = MessageStream::for_match_rule(rule, &conn, None)
        .await
        .map_err(|e| anyhow::anyhow!("failed to subscribe to KWin callbacks: {e}"))?;

    // KWin rejects a second script under the same name, and a previous
    // instance may have left its resident script loaded (crash/kill), so
    // unload first and let the failure fall through.
    let script_path = std::env::temp_dir().join(format!("{SCRIPT_NAME}.js"));

    let _ = tokio::time::timeout(
        SCRIPT_TIMEOUT,
        conn.call_method(
            Some(KW_SCRIPTING_SERVICE),
            KW_SCRIPTING_PATH,
            Some(KW_SCRIPTING_IFACE),
            "unloadScript",
            &(SCRIPT_NAME,),
        ),
    )
    .await;

    let script = format!(
        "function report() {{ var w = workspace.activeWindow; if (w) {{ callDBus(\"{our_name}\", \"/\", \"\", \"{SCRIPT_MEMBER}\", w.resourceClass || w.resourceName || \"\"); }} }}\n\
         workspace.windowActivated.connect(report);\n\
         report();"
    );
    if std::fs::write(&script_path, script).is_err() {
        return Err(anyhow::anyhow!("failed to write tracker script"));
    }

    let loaded: i32 = match tokio::time::timeout(
        SCRIPT_TIMEOUT,
        conn.call_method(
            Some(KW_SCRIPTING_SERVICE),
            KW_SCRIPTING_PATH,
            Some(KW_SCRIPTING_IFACE),
            "loadScript",
            &(script_path.to_string_lossy().as_ref(), SCRIPT_NAME),
        ),
    )
    .await
    {
        Ok(Ok(msg)) => match msg.body().deserialize() {
            Ok(id) => id,
            Err(_) => {
                let _ = std::fs::remove_file(&script_path);
                return Err(anyhow::anyhow!("loadScript returned an unreadable id"));
            }
        },
        Ok(Err(e)) => {
            let _ = std::fs::remove_file(&script_path);
            return Err(anyhow::anyhow!("loadScript failed: {e}"));
        }
        Err(_) => {
            let _ = std::fs::remove_file(&script_path);
            return Err(anyhow::anyhow!("loadScript timed out"));
        }
    };
    if loaded < 0 {
        let _ = std::fs::remove_file(&script_path);
        return Err(anyhow::anyhow!("KWin rejected the tracker script"));
    }

    let script_obj = format!("{KW_SCRIPTING_PATH}/Script{loaded}");

    // `run` has a delayed reply that fires once the script has executed, so
    // awaiting it guarantees the initial `report()` was already issued. A hung
    // KWin must not stall the tracker, so every scripting call is bounded by
    // the same timeout.
    let _ = tokio::time::timeout(
        SCRIPT_TIMEOUT,
        conn.call_method(
            Some(KW_SCRIPTING_SERVICE),
            script_obj.as_str(),
            Some(KW_SCRIPT_IFACE),
            "run",
            &(),
        ),
    )
    .await;

    // KWin opens the script file at `run` time, not load time — deleting it
    // any earlier fails `run` with "Could not open …", leaving the resident
    // script dead (no top-level code, no signal connection) and the tracker
    // hanging on a stream that never sees a callback. The code lives in the
    // engine once `run` completes, so the file is safe to clean up only now.
    let _ = std::fs::remove_file(&script_path);

    // The script now reports every focus change; block on the stream until it
    // dies. There is no timeout here: an idle session legitimately produces no
    // callbacks for a long stretch.
    loop {
        match stream.next().await {
            Some(Ok(msg)) => {
                match msg.body().deserialize::<String>() {
                    Ok(app) => {
                        let app = (!app.is_empty()).then_some(app);
                        if let Ok(mut guard) = cache.write() {
                            *guard = app;
                        }
                    }
                    // A malformed callback says nothing about the active
                    // window; leave the cache untouched rather than clearing it.
                    Err(_) => {}
                }
            }
            Some(Err(e)) => {
                return Err(anyhow::anyhow!("tracker stream error: {e}"));
            }
            // The stream closed (e.g. session bus restarted): re-install from
            // scratch so the script is reloaded and the cache keeps tracking.
            None => return Ok(()),
        }
    }
}
