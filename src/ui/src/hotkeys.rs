use anyhow::{Context, Result};
use ashpd::desktop::CreateSessionOptions;
use ashpd::desktop::global_shortcuts::{BindShortcutsOptions, GlobalShortcuts, NewShortcut};
use futures::StreamExt;
use std::collections::HashMap;
use std::time::Duration;
use tokio::time::timeout;
use tracing::info;
use zbus::{Connection, zvariant::Value};

const PORTAL_DEST: &str = "org.freedesktop.portal.Desktop";
const PORTAL_PATH: &str = "/org/freedesktop/portal/desktop";
const HOST_REGISTRY_IFACE: &str = "org.freedesktop.host.portal.Registry";

/// Application id used to register with the portal. Must match the basename
/// of an installed `.desktop` file (the PKGBUILD installs `cliptoo.desktop`).
const APP_ID: &str = "cliptoo";

/// How long a hotkey value must stay unchanged before a re-registration is
/// triggered (see `wait_for_stable_hotkey`).
const HOTKEY_DEBOUNCE: Duration = Duration::from_millis(800);

/// Map a key token (as displayed in Settings) to the XKB keysym name the
/// portal backend (`xdg-desktop-portal-kde`'s `XdgShortcut::parse`) matches
/// via `xkb_keysym_from_name`. Letters, digits and most named keys (F5,
/// Return, …) round-trip unchanged, but ASCII punctuation the UI reports
/// literally (`+`, `-`, `=`, …) and named keys whose keysym differs from the
/// display name (`PageUp` → `Prior`, `Backspace` → `BackSpace`, …) must be
/// translated — otherwise the key fails to parse and the shortcut is
/// registered with nothing bound.
fn to_xdg_key(key: &str) -> &str {
    match key {
        "+" => "plus",
        "-" => "minus",
        "=" => "equal",
        "/" => "slash",
        "*" => "asterisk",
        "," => "comma",
        "." => "period",
        ";" => "semicolon",
        ":" => "colon",
        "'" => "apostrophe",
        "\"" => "quotedbl",
        "`" => "grave",
        "~" => "asciitilde",
        "!" => "exclam",
        "@" => "at",
        "#" => "numbersign",
        "$" => "dollar",
        "%" => "percent",
        "^" => "asciicircum",
        "&" => "ampersand",
        "(" => "parenleft",
        ")" => "parenright",
        "_" => "underscore",
        "{" => "braceleft",
        "}" => "braceright",
        "[" => "bracketleft",
        "]" => "bracketright",
        "|" => "bar",
        "\\" => "backslash",
        " " => "space",
        "UpArrow" => "Up",
        "DownArrow" => "Down",
        "LeftArrow" => "Left",
        "RightArrow" => "Right",
        "PageUp" => "Prior",
        "PageDown" => "Next",
        "Backtab" => "ISO_Left_Tab",
        "Backspace" => "BackSpace",
        "ScrollLock" => "Scroll_Lock",
        "SysReq" => "Print",
        "Stop" => "XF86_Stop",
        _ => key,
    }
}

/// Convert a Qt-style hotkey string (as stored in settings, e.g. `Ctrl+Alt+z`)
/// to the XDG Shortcuts spec format the portal expects (e.g. `CTRL+ALT+z`).
///
/// The portal backend (xdg-desktop-portal-kde's `XdgShortcut::parse`) matches
/// modifier names case-sensitively against `{SHIFT, CAPS, CTRL, ALT, NUM, LOGO}`,
/// so a mixed-case trigger like `Ctrl+Alt+z` fails to parse and the shortcut is
/// registered with no key bound. Normalise the modifier tokens here instead of
/// changing what the user types in the settings UI.
///
/// A `+` key collides with the `+` separator (`Ctrl++` = Ctrl + the plus key),
/// and the backend rejects a literal `++` ("empty modifier"). The key token is
/// therefore split off from the right and translated to its keysym name, so a
/// `Ctrl++` hotkey becomes `CTRL+plus`.
fn to_xdg_trigger(trigger: &str) -> String {
    // A trailing `+` is the plus key itself, not a separator.
    let (mods_str, key) = match trigger.strip_suffix('+') {
        Some(before) => (before, "+"),
        None => match trigger.rsplit_once('+') {
            Some((m, k)) => (m, k),
            // A single token is the bare key (e.g. `F5`) — nothing to
            // normalise beyond the keysym-name mapping.
            None => return to_xdg_key(trigger).to_string(),
        },
    };

    let mods = mods_str
        .split('+')
        .filter(|m| !m.is_empty())
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
    out.push_str(to_xdg_key(key));
    out
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
/// portable. This function therefore returns without a listener when the
/// XDG Desktop Portal is not running — it logs a warning and returns
/// `Ok(None)` so callers can retry later (see [`run_hotkey_loop`]).
///
/// # Failure modes
///
/// The function gracefully degrades:
///
/// 1. The session bus is unreachable — the only hard-fail case; returns
///    `Err` (a `warn!` accompanies it).
/// 2. `CreateSession`, `BindShortcuts`, or the `Activated`-signal
///    subscription fails (e.g. portal service is absent or hung) — emits a
///    `warn!` and returns `Ok(None)`, meaning no listener was installed.
/// 3. The `BindShortcuts` Response signal reports an error — emits a
///    `warn!`; the function still installs the Activated-signal listener
///    and returns `Ok(Some(handle))` because partial success is possible.
///
/// In every case the app continues to run; only the global hotkey is
/// affected. Callers should pair this function with [`check_portal_presence`]
/// at startup so the user gets an informational heads-up before the
/// session bus is first exercised, and retry a `None` result — the portal
/// is often not yet running when an autostart app launches at login.
pub async fn register_shortcuts_and_listen<F>(
    shortcut_id: &str,
    trigger: &str,
    mut handler: F,
) -> Result<Option<tokio::task::JoinHandle<()>>>
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

    let proxy = GlobalShortcuts::with_connection(conn)
        .await
        .context("GlobalShortcuts portal proxy")?;

    // ── Create session ────────────────────────────────────────────────────
    let session = match timeout(
        Duration::from_secs(10),
        proxy.create_session(CreateSessionOptions::default()),
    )
    .await
    {
        Ok(Ok(s)) => s,
        Ok(Err(e)) => {
            tracing::warn!(
                "Global shortcuts unavailable: {e}. \
                 This requires the XDG Desktop Portal (xdg-desktop-portal) \
                 with GlobalShortcuts support. On KDE Plasma 6, ensure \
                 xdg-desktop-portal-kde is installed and running."
            );
            return Ok(None);
        }
        Err(_) => {
            tracing::warn!("timed out creating a GlobalShortcuts session");
            return Ok(None);
        }
    };

    // ── Bind shortcuts ────────────────────────────────────────────────────
    let trigger = to_xdg_trigger(trigger);
    let shortcut = NewShortcut::new(shortcut_id.to_string(), shortcut_id.to_string())
        .preferred_trigger(Some(trigger.as_str()));
    let bind_request = match timeout(
        Duration::from_secs(10),
        proxy.bind_shortcuts(&session, &[shortcut], None, BindShortcutsOptions::default()),
    )
    .await
    {
        Ok(Ok(r)) => r,
        Ok(Err(e)) => {
            tracing::warn!("BindShortcuts failed (shortcuts may not work): {e}");
            return Ok(None);
        }
        Err(_) => {
            tracing::warn!("timed out binding shortcuts");
            return Ok(None);
        }
    };

    // Log the BindShortcuts response outcome; partial success is possible so
    // the listener is installed regardless.
    match bind_request.response() {
        Ok(_) => info!("registered global shortcut {shortcut_id}: {trigger}"),
        Err(e) => tracing::warn!("BindShortcuts response err (shortcuts may still work): {e}"),
    }

    // ── Listen for Activated signals ──────────────────────────────────────
    let mut activated = match proxy.receive_activated().await {
        Ok(s) => s,
        Err(e) => {
            tracing::warn!("failed to subscribe to Activated signals: {e}");
            return Ok(None);
        }
    };

    let handle = tokio::spawn(async move {
        while let Some(activated) = activated.next().await {
            handler(activated.shortcut_id().to_string());
        }
    });

    Ok(Some(handle))
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

/// How long to wait after a failed registration before retrying. Keeps a
/// late-starting portal from leaving the hotkey unregistered forever: on
/// login, `xdg-desktop-portal` typically starts *after* autostart apps. The
/// delay is long enough to be a silent background retry and short enough
/// that a user who installed/fixed the portal gets the hotkey back promptly.
const REGISTRATION_RETRY_DELAY: Duration = Duration::from_secs(15);

/// Keep the global toggle shortcut registered, re-registering whenever the
/// user changes the hotkey in Settings, the portal restarts (the listener
/// ends on its own), or a previous registration failed (the portal was not
/// yet running).
///
/// The settings UI commits on every key-press, so `wait_for_stable_hotkey`
/// debounces the re-registration until the combo has been stable for a quiet
/// period (so the KDE confirmation dialog sees the complete combo, not the
/// first modifier key).
pub async fn run_hotkey_loop(
    ui: slint::Weak<crate::AppWindow>,
    mut hotkey_rx: tokio::sync::watch::Receiver<String>,
) {
    const TOGGLE_ID: &str = "toggle-cliptoo";

    loop {
        let main_hotkey = hotkey_rx.borrow().clone();

        check_portal_presence().await;

        let handler = {
            let weak = ui.clone();
            move |shortcut_id: String| {
                if shortcut_id == TOGGLE_ID {
                    let _ = weak.upgrade_in_event_loop(move |ui| {
                        crate::window::toggle_window(&ui);
                    });
                }
            }
        };

        let shutdown =
            match register_shortcuts_and_listen(TOGGLE_ID, main_hotkey.as_str(), handler).await {
                Ok(Some(mut handle)) => {
                    // A live listener is installed. Re-register when the user
                    // finishes editing the hotkey (debounced) or the listener
                    // ends on its own (portal restarted / bus dropped).
                    if wait_for_stable_hotkey(&mut hotkey_rx, &mut handle).await {
                        true
                    } else {
                        // Drop the old listener, clear the stale KGlobalAccel
                        // keys so the portal treats the shortcut as new (and
                        // applies the new preferred_trigger), then loop to
                        // re-register.
                        handle.abort();
                        clear_kglobalaccel_bindings(TOGGLE_ID).await;
                        false
                    }
                }
                Ok(None) => {
                    // Registration failed (portal absent or hung): retry in the
                    // background rather than blocking on the hotkey channel, and
                    // re-register sooner if the user edits the hotkey meanwhile.
                    wait_for_retry(&mut hotkey_rx, REGISTRATION_RETRY_DELAY).await
                }
                Err(e) => {
                    tracing::warn!("Global shortcuts unavailable: {e}");
                    wait_for_retry(&mut hotkey_rx, REGISTRATION_RETRY_DELAY).await
                }
            };

        if shutdown {
            break;
        }
    }
}

/// Wait for the user to finish editing the hotkey in Settings (the combo has
/// stayed stable for `HOTKEY_DEBOUNCE`), or for the registration task to end
/// (the portal restarted or the bus dropped — re-register).
///
/// First waits for a change to arrive or the listener to end, then restarts
/// the debounce timer on every further change until the value has stayed
/// stable. Returns `true` when the sender has been dropped (shutdown).
async fn wait_for_stable_hotkey(
    hotkey_rx: &mut tokio::sync::watch::Receiver<String>,
    listener: &mut tokio::task::JoinHandle<()>,
) -> bool {
    // Wait for the first event: a hotkey change or the listener ending. A
    // task that already completed (e.g. the portal closed the session right
    // after binding) resolves immediately here.
    tokio::select! {
        changed = hotkey_rx.changed() => match changed {
            Ok(()) => {}
            Err(_) => return true,
        },
        _ = &mut *listener => {
            tracing::warn!("global shortcut listener ended; re-registering");
            return false;
        }
    }

    loop {
        tokio::select! {
            changed = timeout(HOTKEY_DEBOUNCE, hotkey_rx.changed()) => match changed {
                Err(_) => return false,
                Ok(Err(_)) => return true,
                Ok(Ok(())) => {}
            },
            // A listener that ends mid-edit also triggers an immediate
            // re-register; the pending hotkey value is picked up from the
            // watch channel on the next loop pass.
            _ = &mut *listener => {
                tracing::warn!("global shortcut listener ended; re-registering");
                return false;
            }
        }
    }
}

/// Wait for the next registration attempt: the retry interval, or the user
/// finishing a hotkey edit (debounced), whichever comes first. Returns `true`
/// when the sender has been dropped (shutdown).
async fn wait_for_retry(
    hotkey_rx: &mut tokio::sync::watch::Receiver<String>,
    retry_delay: Duration,
) -> bool {
    tokio::select! {
        changed = hotkey_rx.changed() => match changed {
            Ok(()) => {}
            Err(_) => return true,
        },
        _ = tokio::time::sleep(retry_delay) => return false,
    }

    // The hotkey changed: debounce the rest of the edit, then re-register
    // with the final combo.
    loop {
        match timeout(HOTKEY_DEBOUNCE, hotkey_rx.changed()).await {
            Err(_) => return false,
            Ok(Err(_)) => return true,
            Ok(Ok(())) => {}
        }
    }
}
