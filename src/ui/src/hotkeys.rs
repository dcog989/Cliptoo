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

/// Register `shortcuts` as global hotkeys via the XDG Desktop Portal and
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
/// XDG Desktop Portal is not running — it logs a warning and returns
/// `Ok(())`.
///
/// # Failure modes
///
/// The function gracefully degrades in three places:
///
/// 1. `CreateSession` fails (e.g. portal service is absent or hung) — the
///    `Err` arm at line ~186 emits a `warn!` and returns `Ok(())`.
/// 2. `BindShortcuts` fails — the `Err` arm at line ~243 emits a `warn!`
///    and returns `Ok(())`.
/// 3. The `BindShortcuts` Response signal reports an error — the `Err`
///    arm at line ~263 emits a `warn!`; the function still proceeds to
///    install the Activated-signal listener because partial success is
///    possible.
///
/// In every case the app continues to run; only the global hotkey is
/// affected. Callers should pair this function with [`check_portal_presence`]
/// at startup so the user gets an informational heads-up before the
/// session bus is first exercised.
pub async fn register_shortcuts_and_listen<F>(
    shortcuts: &[(&str, &str)],
    mut handler: F,
) -> Result<()>
where
    F: FnMut(String) + Send + 'static,
{
    let conn = Connection::session()
        .await
        .context("session bus connection")?;

    // ── Create session ────────────────────────────────────────────────────
    // Start listening BEFORE calling CreateSession to avoid race.
    let mut signal_stream = MessageStream::from(&conn);

    let handle_token = format!("cliptoo_req_{}", sanitize_dbus_token(shortcuts[0].0));
    let mut options = HashMap::<&str, Value>::new();
    options.insert("session_handle_token", Value::from("cliptoo_session"));
    options.insert("handle_token", Value::from(handle_token.as_str()));

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
            return Ok(());
        }
    };

    let session_handle =
        expect_response_value(&mut signal_stream, &request_handle, "session_handle", 10)
            .await?
            .context("session_handle not in Response results")?;

    // ── Bind shortcuts ────────────────────────────────────────────────────
    let mut bind_stream = MessageStream::from(&conn);

    let bind_handle_token = format!("cliptoo_bind_{}", sanitize_dbus_token(shortcuts[0].0));
    let mut bind_options = HashMap::<String, Value>::new();
    bind_options.insert(
        "handle_token".into(),
        Value::from(bind_handle_token.as_str()),
    );

    let shortcut_defs: Vec<(&str, HashMap<String, Value>)> = shortcuts
        .iter()
        .map(|(id, trigger)| {
            let mut opts = HashMap::<String, Value>::new();
            opts.insert("description".into(), Value::from(*id));
            opts.insert("preferred_trigger".into(), Value::from(*trigger));
            (*id, opts)
        })
        .collect();

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
            return Ok(());
        }
    };

    // Wait for the BindShortcuts response signal (log outcome, don't fail)
    match expect_response_value(&mut bind_stream, &bind_handle, "shortcuts", 10).await {
        Ok(Some(_)) => {
            info!("registered {} global shortcut(s)", shortcuts.len());
            for (id, trigger) in shortcuts {
                info!("  shortcut {id}: {trigger}");
            }
        }
        Ok(None) => {
            info!("registered shortcuts (no shortcuts detail in response)");
            for (id, trigger) in shortcuts {
                info!("  shortcut {id}: {trigger}");
            }
        }
        Err(e) => {
            tracing::warn!("BindShortcuts response err (shortcuts may still work): {e}");
        }
    }

    // ── Listen for Activated signals ──────────────────────────────────────
    let mut stream = MessageStream::from(&conn);

    tokio::spawn(async move {
        while let Some(Ok(msg)) = stream.next().await {
            let hdr = msg.header();
            let is_shortcut = hdr
                .interface()
                .is_some_and(|i| i.as_str() == SHORTCUT_IFACE);
            let is_activated = hdr.member().is_some_and(|m| m.as_str() == "Activated");
            if is_shortcut && is_activated {
                let raw = msg.body();
                let body: std::result::Result<
                    (OwnedObjectPath, String, u32, HashMap<String, Value>),
                    _,
                > = raw.deserialize();
                if let Ok((_, shortcut_id, _, _)) = body {
                    handler(shortcut_id);
                }
            }
        }
    });

    Ok(())
}
