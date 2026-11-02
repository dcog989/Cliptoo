//! Continuously track the application that owns the active window (the
//! "source app" for a copy) and expose it as a shared, near-real-time cache.
//!
//! KWin 6 removed the `org.kde.KWin.activeWindow` D-Bus method and reworked
//! `getWindowInfo` to take a window `uuid`; the only supported way to learn
//! the active window's application is the KWin scripting interface
//! (`org.kde.KWin` at `/Scripting`). A resident script subscribes to
//! `workspace.windowActivated` and pushes the active window's `resourceClass`
//! back to us with `callDBus` on every focus change; the callbacks arrive on a
//! dedicated session connection via a `MessageStream` match rule.
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
// Prefix for the per-process script name (see `install_tracker`). The suffix
// must differ on every start: KWin only runs a script under a name it has not
// loaded before.
const SCRIPT_NAME_PREFIX: &str = "cliptoo-activewindow";
// Bus name the script calls back on. Fixed, unlike the connection's unique
// name: KWin does not reliably replace the resident script on restart, so the
// loaded script may still be the previous process's — a fixed name makes it
// reach the running instance anyway.
const TRACKER_SERVICE: &str = "org.cliptoo.Cliptoo";
// Re-install cadence when the script can't be loaded or the stream dies
// (e.g. KWin restarted underneath us). The cache keeps its last value during
// the outage, which is better than nothing.
const RETRY_DELAY: Duration = Duration::from_secs(30);
// A first install right after startup can still fail for a transient reason
// (e.g. KWin busy while the session is coming up). Retry quickly once so a
// non-empty blacklist does not drop every clip for the full 30 s.
const FIRST_RETRY_DELAY: Duration = Duration::from_secs(2);

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
            tokio::time::sleep(if consecutive_failures <= 1 {
                FIRST_RETRY_DELAY
            } else {
                RETRY_DELAY
            })
            .await;
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
    // Dedicated connection for the fixed callback name, so a script left over
    // from an earlier process still reaches this instance (KWin does not
    // reliably replace the resident script, and the connection's unique name
    // changes on every restart). Start the object server before owning the
    // name: zbus warns that calls can be lost otherwise.
    let conn = zbus::connection::Builder::session()?.build().await?;
    conn.object_server();
    conn.request_name(TRACKER_SERVICE)
        .await
        .map_err(|e| anyhow::anyhow!("failed to own {TRACKER_SERVICE}: {e}"))?;

    // Subscribe before loading the script so a fast initial `report()` can't
    // race ahead of us. The callback is a method call to `TRACKER_SERVICE` with
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

    // per-process script name: KWin only executes a script the first time a
    // given name is loaded in its session. Reusing one name makes every start
    // after the first a silent no-op (the old handler keeps running), which is
    // exactly what killed tracking across restarts. Leftover scripts from
    // earlier processes keep reporting to the same fixed TRACKER_SERVICE, so
    // they are harmless duplicates until the next start unloads them.
    // shortcut: a crash before the name is recorded below leaks one inert
    // script until KWin restarts.
    let script_name = format!("{SCRIPT_NAME_PREFIX}-{}", std::process::id());
    let runtime_dir = cliptoo_core::temp::ensure_runtime_dir().await?;
    let script_path = runtime_dir.join(format!("{script_name}.js"));

    // KWin cannot list loaded scripts, so the previous run's script can only be
    // removed by the name it was loaded under. Remember it in the runtime dir
    // (same lifetime as KWin's script engine) and unload it before loading this
    // process's script.
    let previous_path = runtime_dir.join(format!("{SCRIPT_NAME_PREFIX}.previous"));
    if let Ok(previous) = std::fs::read_to_string(&previous_path) {
        let previous = previous.trim();
        if !previous.is_empty() && previous != script_name {
            let _ = tokio::time::timeout(
                SCRIPT_TIMEOUT,
                conn.call_method(
                    Some(KW_SCRIPTING_SERVICE),
                    KW_SCRIPTING_PATH,
                    Some(KW_SCRIPTING_IFACE),
                    "unloadScript",
                    &(previous,),
                ),
            )
            .await;
        }
    }
    if let Err(e) = std::fs::write(&previous_path, &script_name) {
        tracing::warn!("tracker: cannot record script name {previous_path:?}: {e}");
    }

    let script = format!(
        "function report() {{ var w = workspace.activeWindow; if (w) {{ callDBus(\"{TRACKER_SERVICE}\", \"/\", \"\", \"{SCRIPT_MEMBER}\", w.resourceClass || w.resourceName || \"\"); }} }}\n\
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
            &(script_path.to_string_lossy().as_ref(), script_name.as_str()),
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

    // A never-used script name runs immediately and its opening `report()`
    // arrives (verified with a probe script), so the cache already holds the
    // current app before the first copy.
    debug!("active-window tracker installed");

    // Block on the stream until it dies. There is no timeout here: an idle
    // session legitimately produces no callbacks for a long stretch.
    loop {
        match stream.next().await {
            Some(Ok(msg)) => apply_callback(cache, msg),
            Some(Err(e)) => {
                return Err(anyhow::anyhow!("tracker stream error: {e}"));
            }
            // The stream closed (e.g. session bus restarted): re-install from
            // scratch so the script is reloaded and the cache keeps tracking.
            None => return Ok(()),
        }
    }
}

/// Apply one KWin tracker callback to the cache. A malformed callback says
/// nothing about the active window, so the cache is left untouched rather than
/// cleared.
fn apply_callback(cache: &ActiveWindowCache, msg: zbus::Message) {
    let Ok(app) = msg.body().deserialize::<String>() else {
        return;
    };
    let app = (!app.is_empty()).then_some(app);
    if let Ok(mut guard) = cache.write() {
        *guard = app;
    }
}
