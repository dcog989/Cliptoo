use anyhow::{Context, Result};
use futures::StreamExt;
use std::collections::HashMap;
use std::time::Duration;
use tokio::time::timeout;
use tracing::info;
use zbus::{
    Connection, MessageStream,
    zvariant::{OwnedObjectPath, OwnedValue, Value},
};

const PORTAL_DEST: &str = "org.freedesktop.portal.Desktop";
const PORTAL_PATH: &str = "/org/freedesktop/portal/desktop";
const SHORTCUT_IFACE: &str = "org.freedesktop.portal.GlobalShortcuts";
const REQUEST_IFACE: &str = "org.freedesktop.portal.Request";
const HOST_REGISTRY_IFACE: &str = "org.freedesktop.host.portal.Registry";

/// Application id used to register with the portal. Must match the basename
/// of an installed `.desktop` file (the PKGBUILD installs `cliptoo.desktop`).
const APP_ID: &str = "cliptoo";

/// Replace characters that are invalid in D-Bus object path elements with `_`.
/// Valid chars are `[A-Za-z0-9_]`.
fn sanitize_dbus_token(s: &str) -> String {
    s.chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

/// Convert a Qt-style hotkey string (as stored in settings, e.g. `Ctrl+Alt+z`)
/// to the XDG Shortcuts spec format the portal expects (e.g. `CTRL+ALT+z`).
///
/// The portal backend (xdg-desktop-portal-kde's `XdgShortcut::parse`) matches
/// modifier names case-sensitively against `{SHIFT, CAPS, CTRL, ALT, NUM, LOGO}`,
/// so a mixed-case trigger like `Ctrl+Alt+z` fails to parse and the shortcut is
/// registered with no key bound. Normalise the modifier tokens here instead of
/// changing what the user types in the settings UI.
fn to_xdg_trigger(trigger: &str) -> String {
    let parts: Vec<&str> = trigger.split('+').collect();

    // A single token is the bare key (e.g. `F5`) — nothing to normalise.
    if parts.len() == 1 {
        return trigger.to_string();
    }

    let mods = parts[..parts.len() - 1]
        .iter()
        .map(|m| match m.to_ascii_uppercase().as_str() {
            "CTRL" | "CONTROL" => "CTRL".to_string(),
            "SHIFT" => "SHIFT".to_string(),
            "CAPS" | "CAPSLOCK" => "CAPS".to_string(),
            "ALT" => "ALT".to_string(),
            "NUM" | "NUMLOCK" => "NUM".to_string(),
            "META" | "SUPER" | "LOGO" | "WIN" => "LOGO".to_string(),
            other => other.to_string(),
        })
        .collect::<Vec<_>>();

    let mut out = mods.join("+");
    if !out.is_empty() {
        out.push('+');
    }
    out.push_str(parts[parts.len() - 1]);
    out
}

/// Wait for a portal Response signal on `request_handle`, then extract
/// `key` from the results dict as a String.  Returns `None` if the key is
/// absent, `Err` on D-Bus / timeout / protocol error.
async fn expect_response_value(
    stream: &mut MessageStream,
    request_handle: &OwnedObjectPath,
    key: &str,
    timeout_secs: u64,
) -> Result<Option<String>> {
    let deadline = timeout(Duration::from_secs(timeout_secs), async {
        while let Some(Ok(msg)) = stream.next().await {
            let hdr = msg.header();
            let on_path = hdr.path().is_some_and(|p| p == request_handle.as_str());
            let on_iface = hdr.interface().is_some_and(|i| i.as_str() == REQUEST_IFACE);
            let is_response = hdr.member().is_some_and(|m| m.as_str() == "Response");
            if !(on_path && on_iface && is_response) {
                continue;
            }

            // Deserialise into owned values so we can return them.
            let raw = msg.body();
            let (code, results): (u32, HashMap<String, OwnedValue>) =
                raw.deserialize().context("parse Response body")?;

            if code != 0 {
                anyhow::bail!("portal rejected request (code {code})");
            }

            return Ok(results
                .get(key)
                .and_then(|v| v.downcast_ref::<&str>().ok().map(|s| s.to_string())));
        }
        anyhow::bail!("message stream ended")
    })
    .await
    .context("timeout waiting for portal response")?
    .context("portal response error")?;
    Ok(deadline)
}

/// One-shot, best-effort probe for the XDG Desktop Portal at startup.
///
/// On Wayland, the portal is the **only** mechanism a client app can use to
/// register a global keyboard shortcut. Compositors do not expose key-grab
/// APIs (X11's `XGrabKey` is unavailable to Wayland clients) and apps like
/// KDE's `KGlobalAccel` (used by CopyQ on KDE) are out of reach for
/// non-KDE-toolkit clients.
///
/// The portal stack is layered:
///
/// ```text
///   client  ──▶  xdg-desktop-portal           (D-Bus router)
///                   │
///                   └─▶  xdg-desktop-portal-kde     (KDE Plasma 6 backend)
///                       xdg-desktop-portal-gnome   (GNOME backend)
///                       xdg-desktop-portal-wlr     (wlroots backend)
///                       xdg-desktop-portal-gtk     (GTK fallback)
/// ```
///
/// Cliptoo's PKGBUILD declares `xdg-desktop-portal-kde` so a normal install
/// gets the router + backend transitively. If this check fires, the user is
/// either running outside a supported desktop session, the portal service
/// has been stopped, or the binary is being run on a system where the
/// portal was never installed. The app continues to run; only the global
/// hotkey is unavailable.
pub async fn check_portal_presence() {
    let conn = match Connection::session().await {
        Ok(c) => c,
        Err(e) => {
            info!(
                "no D-Bus session bus ({e}); global hotkey will not be registered. \
                 The XDG Desktop Portal is required to register a global hotkey on \
                 Wayland. Ensure the binary is launched inside a desktop session."
            );
            return;
        }
    };

    // 5s is generous; NameHasOwner is a synchronous bus call that should
    // resolve in microseconds on a working system. The timeout exists so
    // a hung D-Bus daemon cannot block startup indefinitely.
    let reply = timeout(
        Duration::from_secs(5),
        conn.call_method(
            Some("org.freedesktop.DBus"),
            "/org/freedesktop/DBus",
            Some("org.freedesktop.DBus"),
            "NameHasOwner",
            &("org.freedesktop.portal.Desktop",),
        ),
    )
    .await;

    let owned = match reply {
        Ok(Ok(msg)) => msg.body().deserialize::<bool>().unwrap_or(false),
        Ok(Err(e)) => {
            info!(
                "D-Bus NameHasOwner for org.freedesktop.portal.Desktop failed: {e}; \
                 global hotkey will not be registered."
            );
            return;
        }
        Err(_) => {
            info!(
                "timed out querying D-Bus for org.freedesktop.portal.Desktop; \
                 global hotkey will not be registered."
            );
            return;
        }
    };

    if !owned {
        info!(
            "XDG Desktop Portal (org.freedesktop.portal.Desktop) is not running; \
             the global hotkey will not be registered. On Wayland the portal is \
             the only mechanism for client apps to register global hotkeys. \
             Install the appropriate backend for your compositor: \
             `xdg-desktop-portal-kde` (KDE Plasma 6), `xdg-desktop-portal-gnome` \
             (GNOME), `xdg-desktop-portal-wlr` (Sway/Hyprland/etc.), or \
             `xdg-desktop-portal-gtk` (other GTK environments). The app \
             continues to run; only the toggle hotkey is affected."
        );
    }
}

/// Register a single global shortcut via the XDG Desktop Portal and
/// dispatch activations to `handler`.
///
/// # Platform note — Wayland only
///
/// On Wayland, the portal's `org.freedesktop.portal.GlobalShortcuts`
/// interface is the **only** sanctioned mechanism for a client app to
/// register a global keyboard shortcut. Compositors do not expose key-grab
/// APIs to Wayland clients (the X11 `XGrabKey` / `xcb_grab_key` path is
/// not available), and D-Bus interfaces specific to other toolkits
/// (KDE's `KGlobalAccel`, GNOME's `org.gnome.Shell` keybindings) are not
/// portable. This function is therefore a no-op on a system where the
/// XDG Desktop Portal is not running — it logs a warning and returns a
/// completed `JoinHandle` so callers can always abort the listener.
///
/// # Failure modes
///
/// The function gracefully degrades in three places:
///
/// 1. `CreateSession` fails (e.g. portal service is absent or hung) — the
///    `Err` arm emits a `warn!` and returns a completed `JoinHandle`.
/// 2. `BindShortcuts` fails — the `Err` arm emits a `warn!` and returns a
///    completed `JoinHandle`.
/// 3. The `BindShortcuts` Response signal reports an error — the `Err`
///    arm emits a `warn!`; the function still proceeds to
///    install the Activated-signal listener because partial success is
///    possible.
///
/// In every case the app continues to run; only the global hotkey is
/// affected. Callers should pair this function with [`check_portal_presence`]
/// at startup so the user gets an informational heads-up before the
/// session bus is first exercised.
pub async fn register_shortcuts_and_listen<F>(
    shortcut_id: &str,
    trigger: &str,
    mut handler: F,
) -> Result<tokio::task::JoinHandle<()>>
where
    F: FnMut(String) + Send + 'static,
{
    let conn = Connection::session()
        .await
        .context("session bus connection")?;

    // ── Register app id ───────────────────────────────────────────────────
    // The portal rejects `CreateSession` with "An app id is required" unless
    // it can identify the calling app. For an unsandboxed host app the only
    // portable way to supply one is the host Registry interface; the app id
    // must match an installed `.desktop` file basename. This MUST be the
    // first portal call on this connection, so it is done before anything
    // else. Best-effort: if registration fails the request proceeds and the
    // portal will reject it on its own (same net effect, logged below).
    let reg_options = HashMap::<&str, Value>::new();
    match conn
        .call_method(
            Some(PORTAL_DEST),
            PORTAL_PATH,
            Some(HOST_REGISTRY_IFACE),
            "Register",
            &(APP_ID, &reg_options),
        )
        .await
    {
        Ok(_) => info!("registered app id {APP_ID} with the portal"),
        Err(e) => {
            tracing::warn!(
                "failed to register app id {APP_ID} with the portal: {e}. \
                 The XDG Desktop Portal requires an app id to register a \
                 global shortcut; ensure cliptoo is installed with its \
                 `.desktop` file (matching `{APP_ID}.desktop`)."
            );
        }
    }

    // ── Create session ────────────────────────────────────────────────────
    // Start listening BEFORE calling CreateSession to avoid race.
    let mut signal_stream = MessageStream::from(&conn);

    let handle_token = format!("cliptoo_req_{}", sanitize_dbus_token(shortcut_id));
    let mut options = HashMap::<&str, Value>::new();
    options.insert("session_handle_token", Value::from("cliptoo_session"));
    options.insert("handle_token", Value::from(handle_token.as_str()));
    options.insert("desktop-file-name", Value::from("cliptoo"));
    options.insert("application-id", Value::from("org.cliptoo.Cliptoo"));

    let result = conn
        .call_method(
            Some(PORTAL_DEST),
            PORTAL_PATH,
            Some(SHORTCUT_IFACE),
            "CreateSession",
            &(&options),
        )
        .await;

    let request_handle = match result {
        Ok(msg) => {
            let raw = msg.body();
            raw.deserialize::<OwnedObjectPath>()
                .context("parse CreateSession reply")?
        }
        Err(e) => {
            tracing::warn!(
                "Global shortcuts unavailable: {e}. \
                 This requires the XDG Desktop Portal (xdg-desktop-portal) \
                 with GlobalShortcuts support. On KDE Plasma 6, ensure \
                 xdg-desktop-portal-kde is installed and running."
            );
            return Ok(tokio::spawn(async {}));
        }
    };

    let session_handle =
        expect_response_value(&mut signal_stream, &request_handle, "session_handle", 10)
            .await?
            .context("session_handle not in Response results")?;

    // ── Bind shortcuts ────────────────────────────────────────────────────
    let mut bind_stream = MessageStream::from(&conn);

    let bind_handle_token = format!("cliptoo_bind_{}", sanitize_dbus_token(shortcut_id));
    let mut bind_options = HashMap::<String, Value>::new();
    bind_options.insert(
        "handle_token".into(),
        Value::from(bind_handle_token.as_str()),
    );

    let mut shortcut_opts = HashMap::<String, Value>::new();
    shortcut_opts.insert("description".into(), Value::from(shortcut_id));
    shortcut_opts.insert(
        "preferred_trigger".into(),
        Value::from(to_xdg_trigger(trigger)),
    );
    let shortcut_defs = vec![(shortcut_id, shortcut_opts)];

    let session_op =
        OwnedObjectPath::try_from(session_handle.as_str()).context("invalid session handle")?;
    let bind_result = conn
        .call_method(
            Some(PORTAL_DEST),
            PORTAL_PATH,
            Some(SHORTCUT_IFACE),
            "BindShortcuts",
            &(&session_op, &shortcut_defs, "", &bind_options),
        )
        .await;

    let bind_handle = match bind_result {
        Ok(msg) => msg
            .body()
            .deserialize::<OwnedObjectPath>()
            .context("parse BindShortcuts reply")?,
        Err(e) => {
            tracing::warn!("BindShortcuts failed (shortcuts may not work): {e}");
            return Ok(tokio::spawn(async {}));
        }
    };

    // Wait for the BindShortcuts response signal (log outcome, don't fail)
    match expect_response_value(&mut bind_stream, &bind_handle, "shortcuts", 10).await {
        Ok(_) => info!("registered global shortcut {shortcut_id}: {trigger}"),
        Err(e) => tracing::warn!("BindShortcuts response err (shortcuts may still work): {e}"),
    }

    // ── Listen for Activated signals ──────────────────────────────────────
    let mut stream = MessageStream::from(&conn);

    let handle = tokio::spawn(async move {
        while let Some(Ok(msg)) = stream.next().await {
            let hdr = msg.header();
            let is_shortcut = hdr
                .interface()
                .is_some_and(|i| i.as_str() == SHORTCUT_IFACE);
            let is_activated = hdr.member().is_some_and(|m| m.as_str() == "Activated");
            if is_shortcut && is_activated {
                let raw = msg.body();
                let body: std::result::Result<
                    (OwnedObjectPath, String, u64, HashMap<String, Value>),
                    _,
                > = raw.deserialize();
                if let Ok((_, shortcut_id, _, _)) = body {
                    handler(shortcut_id);
                }
            }
        }
    });

    Ok(handle)
}

/// Best-effort: remove `action` from KGlobalAccel so a changed hotkey takes
/// effect.
///
/// The portal's `BindShortcuts` keeps the keys already stored for a shortcut
/// that exists in KGlobalAccel (it treats it as "returning" and restores the
/// stored keys, ignoring the new `preferred_trigger`). Removing the action
/// first makes the portal see it as new and apply the changed trigger.
/// KDE-specific; on other backends this is a harmless no-op.
pub async fn clear_kglobalaccel_bindings(action: &str) {
    const KGLOBALACCEL_DEST: &str = "org.kde.kglobalaccel";
    const KGLOBALACCEL_PATH: &str = "/kglobalaccel";
    const KGLOBALACCEL_IFACE: &str = "org.kde.KGlobalAccel";

    let Ok(conn) = Connection::session().await else {
        return;
    };
    let _ = conn
        .call_method(
            Some(KGLOBALACCEL_DEST),
            KGLOBALACCEL_PATH,
            Some(KGLOBALACCEL_IFACE),
            "unregister",
            &(APP_ID, action),
        )
        .await;
}

/// Keep the global toggle shortcut registered, re-registering whenever the
/// user changes the hotkey in Settings.
///
/// The settings UI commits on every key-press, so typing `Ctrl+Alt+Q` fires
/// several `watch` updates in quick succession. Debounce: only act once the
/// value has been stable for a quiet period, so the KDE confirmation dialog
/// appears for the complete combo, not the first modifier key.
pub async fn run_hotkey_loop(
    ui: slint::Weak<crate::AppWindow>,
    mut hotkey_rx: tokio::sync::watch::Receiver<String>,
) {
    const HOTKEY_DEBOUNCE_MS: u64 = 800;
    const TOGGLE_ID: &str = "toggle-cliptoo";

    loop {
        let main_hotkey = hotkey_rx.borrow().clone();

        check_portal_presence().await;

        let handle = register_shortcuts_and_listen(TOGGLE_ID, main_hotkey.as_str(), {
            let weak = ui.clone();
            move |shortcut_id| {
                if shortcut_id == TOGGLE_ID {
                    let _ = weak.upgrade_in_event_loop(move |ui| {
                        crate::window::toggle_window(&ui);
                    });
                }
            }
        })
        .await;

        if let Err(e) = &handle {
            tracing::warn!("Global shortcuts unavailable: {e}");
        }

        // Wait for the user to change a hotkey in Settings.
        if hotkey_rx.changed().await.is_err() {
            break;
        }
        loop {
            match tokio::time::timeout(
                std::time::Duration::from_millis(HOTKEY_DEBOUNCE_MS),
                hotkey_rx.changed(),
            )
            .await
            {
                Err(_) => break,
                Ok(Err(_)) => break,
                Ok(Ok(())) => continue,
            }
        }

        // Drop the old listener, clear the stale KGlobalAccel keys so the
        // portal treats the shortcut as new (and applies the new
        // preferred_trigger), then loop to re-register.
        handle.map(|h| h.abort()).ok();
        clear_kglobalaccel_bindings(TOGGLE_ID).await;
    }
}
