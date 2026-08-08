//! Detect the application that owns the active window (the "source app" for
//! the current clipboard content).
//!
//! KWin 6 removed the `org.kde.KWin.activeWindow` D-Bus method and reworked
//! `getWindowInfo` to take a window `uuid`; the only supported way to learn
//! the active window's application is the KWin scripting interface
//! (`org.kde.KWin` at `/Scripting`). We load a one-shot script that reads
//! `workspace.activeWindow.resourceClass` and sends it back to us with
//! `callDBus`; the callback is received on the shared session connection via
//! a `MessageStream` match rule (zbus broadcasts every incoming message to
//! all matching subscribers, so this doesn't disturb the object server).
//!
//! This is the same mechanism the `kdotool` crate uses, implemented with
//! zbus so no second D-Bus stack is pulled in.

use std::sync::atomic::{AtomicU64, Ordering};
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

/// The active window's application id (its `resourceClass`), or `None` if it
/// can't be determined (no KWin, no active window, scripting unavailable).
pub async fn detect_source_app() -> Option<String> {
    try_kwin_active_window().await
}

async fn try_kwin_active_window() -> Option<String> {
    let conn = crate::dbus::session().await.ok()?;
    let our_name = conn.unique_name()?.to_string();

    // Subscribe to the script's callback before loading it so a fast reply
    // can't race ahead of us. The callback is a method call to our unique
    // name with member "result".
    let rule = MatchRule::builder()
        .msg_type(Type::MethodCall)
        .member("result")
        .ok()?
        .build();
    let mut stream = MessageStream::for_match_rule(rule, &conn, None)
        .await
        .ok()?;

    // KWin rejects a second script with the same name, and we always unload
    // after a run, so make the name unique per call to survive a failed
    // teardown on a previous invocation.
    let suffix = format!(
        "{}-{}-{}",
        std::process::id(),
        unique_counter(),
        now_nanos()
    );
    let script_name = format!("cliptoo-sourceapp-{suffix}");
    let script_path = std::env::temp_dir().join(format!("{script_name}.js"));

    let script = format!(
        "var w = workspace.activeWindow;\n\
         if (w) {{ callDBus(\"{our_name}\", \"/\", \"\", \"result\", w.resourceClass || w.resourceName || \"\"); }}"
    );
    if std::fs::write(&script_path, script).is_err() {
        return None;
    }

    let loaded: i32 = match conn
        .call_method(
            Some(KW_SCRIPTING_SERVICE),
            KW_SCRIPTING_PATH,
            Some(KW_SCRIPTING_IFACE),
            "loadScript",
            &(script_path.to_string_lossy().as_ref(), script_name.as_str()),
        )
        .await
    {
        Ok(msg) => match msg.body().deserialize() {
            Ok(id) => id,
            Err(_) => {
                let _ = std::fs::remove_file(&script_path);
                return None;
            }
        },
        Err(_) => {
            let _ = std::fs::remove_file(&script_path);
            return None;
        }
    };
    if loaded < 0 {
        let _ = std::fs::remove_file(&script_path);
        return None;
    }

    let script_obj = format!("{KW_SCRIPTING_PATH}/Script{loaded}");
    let script_obj = script_obj.as_str();

    // `run` has a delayed reply that fires once the script has executed, so
    // awaiting it guarantees the callback was already issued by the time we
    // read the stream below.
    let _ = conn
        .call_method(
            Some(KW_SCRIPTING_SERVICE),
            script_obj,
            Some(KW_SCRIPT_IFACE),
            "run",
            &(),
        )
        .await;

    // Stop and unload whether or not we got a usable answer.
    let _ = conn
        .call_method(
            Some(KW_SCRIPTING_SERVICE),
            script_obj,
            Some(KW_SCRIPT_IFACE),
            "stop",
            &(),
        )
        .await;
    let _ = conn
        .call_method(
            Some(KW_SCRIPTING_SERVICE),
            KW_SCRIPTING_PATH,
            Some(KW_SCRIPTING_IFACE),
            "unloadScript",
            &(script_name.as_str(),),
        )
        .await;
    let _ = std::fs::remove_file(&script_path);

    let msg = match tokio::time::timeout(SCRIPT_TIMEOUT, stream.next()).await {
        Ok(Some(Ok(msg))) => msg,
        _ => return None,
    };

    let app: String = msg.body().deserialize().ok()?;
    if app.is_empty() {
        None
    } else {
        debug!("detected source app: {app}");
        Some(app)
    }
}

fn unique_counter() -> u64 {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    COUNTER.fetch_add(1, Ordering::Relaxed)
}

fn now_nanos() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0)
}
