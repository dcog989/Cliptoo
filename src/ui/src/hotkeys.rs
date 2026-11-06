use anyhow::{Context, Result};
use ashpd::desktop::CreateSessionOptions;
use ashpd::desktop::global_shortcuts::{BindShortcutsOptions, GlobalShortcuts, NewShortcut};
use futures::StreamExt;
use std::collections::HashMap;
use std::time::Duration;
use tokio::time::timeout;
use tracing::info;
use zbus::message::Type;
use zbus::{Connection, MatchRule, MessageStream, Variant};

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

/// Split a hotkey into its modifier part and its key token. A trailing `+` is
/// the plus key itself, not a separator.
fn split_trigger(trigger: &str) -> (&str, &str) {
    match trigger.strip_suffix('+') {
        Some(before) => (before, "+"),
        None => match trigger.rsplit_once('+') {
            Some((mods, key)) => (mods, key),
            // A single token is the bare key (e.g. `F5`).
            None => ("", trigger),
        },
    }
}

/// Tokens that name a modifier rather than a key. A trigger that ends in one
/// has no key to bind: the portal accepts it, binds nothing, and reports
/// success, so the hotkey silently never fires.
const MODIFIER_NAMES: [&str; 13] = [
    "CTRL", "CONTROL", "SHIFT", "ALT", "ALTGR", "META", "SUPER", "LOGO", "WIN", "CAPS", "CAPSLOCK",
    "NUM", "NUMLOCK",
];

/// True when the trigger ends in a real key. `clean_hotkey_text` turns a
/// capture with no key (`Ctrl+`) into `CTRL`, and a hand-edited settings file
/// can hold the same, so this is checked before touching the portal.
fn has_bindable_key(trigger: &str) -> bool {
    let key = split_trigger(trigger).1;
    !key.is_empty() && !MODIFIER_NAMES.iter().any(|m| key.eq_ignore_ascii_case(m))
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
    let (mods_str, key) = split_trigger(trigger);

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

/// Stream with one item per owner change of `org.freedesktop.portal.Desktop`
/// — a start, a crash or a package-upgrade restart.
async fn portal_owner_changes(conn: &Connection) -> Result<MessageStream> {
    let rule = MatchRule::builder()
        .msg_type(Type::Signal)
        .sender("org.freedesktop.DBus")
        .path("/org/freedesktop/DBus")
        .interface("org.freedesktop.DBus")
        .member("NameOwnerChanged")
        .add_arg(PORTAL_DEST)
        .build()?;
    Ok(MessageStream::for_match_rule(rule, conn, None).await?)
}

/// Register a single global shortcut via the XDG Desktop Portal and
/// dispatch activations to `handler`.
///
/// `description` is the user-readable label the portal backend shows for the
/// shortcut (e.g. in KDE's "Configure Shortcuts" dialog); `shortcut_id` is
/// the machine-readable identifier used in `Activated` signals and to look
/// up the action in KGlobalAccel.
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
/// affected. Callers should retry a `None` result — the portal is often not
/// yet running when an autostart app launches at login.
pub async fn register_shortcuts_and_listen<F>(
    shortcut_id: &str,
    description: &str,
    trigger: &str,
    mut handler: F,
) -> Result<Option<tokio::task::JoinHandle<()>>>
where
    F: FnMut(String) + Send + 'static,
{
    if !has_bindable_key(trigger) {
        tracing::warn!(
            "hotkey '{trigger}' has no key to bind; keeping the previous shortcut \
             and waiting for a valid one"
        );
        return Ok(None);
    }

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
    let reg_options = HashMap::<&str, Variant>::new();
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

    // A portal restart destroys every session, but zbus keeps this process's
    // signal streams open across the bus-name owner change (it follows the new
    // owner), so the Activated stream would wait forever on a session that no
    // longer exists. Watch the portal name and end the listener when it
    // changes, so `run_hotkey_loop` registers a fresh session.
    let mut portal_owner_changed = match portal_owner_changes(&conn).await {
        Ok(stream) => stream,
        Err(e) => {
            tracing::warn!("cannot watch the portal bus name: {e}");
            return Ok(None);
        }
    };

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
    let shortcut = NewShortcut::new(shortcut_id.to_string(), description.to_string())
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
        loop {
            tokio::select! {
                signal = activated.next() => match signal {
                    Some(activated) => handler(activated.shortcut_id().to_string()),
                    None => break,
                },
                change = portal_owner_changed.next() => match change {
                    Some(Ok(_)) => {
                        tracing::warn!(
                            "the XDG Desktop Portal restarted; re-registering the global shortcut"
                        );
                        break;
                    }
                    Some(Err(e)) => {
                        tracing::warn!("portal bus-name watch failed: {e}; re-registering");
                        break;
                    }
                    None => break,
                },
            }
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

/// Ceiling for the retry delay. Without it a machine with no portal backend
/// would query the portal forever at the base rate; with it the cost settles
/// at one attempt every few minutes.
const MAX_REGISTRATION_RETRY_DELAY: Duration = Duration::from_secs(300);

/// True when the combo actually changed, so the KGlobalAccel keys must be
/// cleared for the portal to apply the new trigger. A listener that merely
/// ended (portal restart, bus drop) must keep the keys the user may have set
/// in KDE System Settings.
fn binding_needs_clearing(previous: &str, current: &str) -> bool {
    previous != current
}

/// Keep the global toggle shortcut registered, re-registering whenever the
/// user changes the hotkey in Settings, the portal restarts (the bus-name
/// watch in [`register_shortcuts_and_listen`] ends the listener), or a
/// previous registration failed (the portal was not yet running).
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
    // User-visible label for the shortcut shown by the portal backend (e.g.
    // KDE's "Configure Shortcuts" dialog).
    const TOGGLE_DESCRIPTION: &str = "Toggle the Cliptoo window";

    let mut retry_delay = REGISTRATION_RETRY_DELAY;
    let mut failed_hotkey: Option<String> = None;

    loop {
        let main_hotkey = hotkey_rx.borrow().clone();

        let handler = {
            let weak = ui.clone();
            move |shortcut_id: String| {
                if shortcut_id == TOGGLE_ID {
                    let _ = weak.upgrade_in_event_loop(move |ui| {
                        // Re-map instead of toggling: a minimized window's
                        // state cannot be trusted, and the shortcut must always
                        // bring the window back focused.
                        crate::window::summon_window(&ui);
                    });
                }
            }
        };

        let shutdown = match register_shortcuts_and_listen(
            TOGGLE_ID,
            TOGGLE_DESCRIPTION,
            main_hotkey.as_str(),
            handler,
        )
        .await
        {
            Ok(Some(mut handle)) => {
                retry_delay = REGISTRATION_RETRY_DELAY;
                failed_hotkey = None;
                // A live listener is installed. Re-register when the user
                // finishes editing the hotkey (debounced) or the listener
                // ends on its own (portal restarted / bus dropped).
                if wait_for_stable_hotkey(&mut hotkey_rx, &mut handle).await {
                    true
                } else {
                    // Drop the old listener. Clear the KGlobalAccel keys only
                    // when the trigger changed, so the portal treats the
                    // shortcut as new and applies it; an unrequested
                    // re-register keeps the keys stored in KDE.
                    handle.abort();
                    if binding_needs_clearing(&main_hotkey, hotkey_rx.borrow().as_str()) {
                        clear_kglobalaccel_bindings(TOGGLE_ID).await;
                    }
                    false
                }
            }
            failed => {
                if let Err(e) = failed {
                    tracing::warn!("Global shortcuts unavailable: {e}");
                }
                // Registration failed (portal absent or hung): retry in the
                // background rather than blocking on the hotkey channel, and
                // back off while the same hotkey keeps failing so a machine
                // without a portal is not polled every 15 seconds forever.
                retry_delay = if failed_hotkey.as_deref() == Some(main_hotkey.as_str()) {
                    (retry_delay * 2).min(MAX_REGISTRATION_RETRY_DELAY)
                } else {
                    REGISTRATION_RETRY_DELAY
                };
                failed_hotkey = Some(main_hotkey);
                wait_for_retry(&mut hotkey_rx, retry_delay).await
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn to_xdg_trigger_normalises_modifiers() {
        assert_eq!(to_xdg_trigger("Ctrl+Alt+z"), "CTRL+ALT+z");
        assert_eq!(to_xdg_trigger("Control+Super+p"), "CTRL+LOGO+p");
        assert_eq!(to_xdg_trigger("Shift+Win+F5"), "SHIFT+LOGO+F5");
    }

    #[test]
    fn to_xdg_trigger_leaves_bare_keys_alone() {
        assert_eq!(to_xdg_trigger("F5"), "F5");
        assert_eq!(to_xdg_trigger("a"), "a");
    }

    #[test]
    fn to_xdg_trigger_translates_punctuation_keys() {
        assert_eq!(to_xdg_trigger("Ctrl++"), "CTRL+plus");
        assert_eq!(to_xdg_trigger("+"), "plus");
        assert_eq!(to_xdg_trigger("Ctrl+-"), "CTRL+minus");
        assert_eq!(to_xdg_trigger("Ctrl+="), "CTRL+equal");
        assert_eq!(to_xdg_trigger("Ctrl+."), "CTRL+period");
    }

    #[test]
    fn to_xdg_trigger_maps_named_keys_to_keysyms() {
        assert_eq!(to_xdg_trigger("Ctrl+Backspace"), "CTRL+BackSpace");
        assert_eq!(to_xdg_trigger("Ctrl+PageUp"), "CTRL+Prior");
        assert_eq!(to_xdg_trigger("Ctrl+UpArrow"), "CTRL+Up");
        assert_eq!(to_xdg_trigger("Ctrl+ScrollLock"), "CTRL+Scroll_Lock");
        assert_eq!(to_xdg_trigger("Ctrl+F12"), "CTRL+F12");
    }

    #[test]
    fn split_trigger_keeps_a_trailing_plus_as_the_key() {
        assert_eq!(split_trigger("Ctrl+Alt+Q"), ("Ctrl+Alt", "Q"));
        assert_eq!(split_trigger("F5"), ("", "F5"));
        assert_eq!(split_trigger("Ctrl++"), ("Ctrl+", "+"));
        assert_eq!(split_trigger("+"), ("", "+"));
    }

    #[test]
    fn has_bindable_key_rejects_bare_modifiers() {
        assert!(has_bindable_key("Ctrl+Alt+Q"));
        assert!(has_bindable_key("Ctrl++"));
        assert!(has_bindable_key("F5"));
        // What `clean_hotkey_text` makes of a capture with no key.
        assert!(!has_bindable_key("CTRL"));
        assert!(!has_bindable_key("Ctrl+Shift"));
        assert!(!has_bindable_key(""));
    }

    #[test]
    fn binding_needs_clearing_only_when_the_hotkey_changed() {
        assert!(!binding_needs_clearing("Ctrl+Alt+Q", "Ctrl+Alt+Q"));
        assert!(binding_needs_clearing("Ctrl+Alt+Q", "Ctrl+Shift+K"));
    }

    #[tokio::test]
    async fn wait_for_retry_returns_after_delay() {
        let (_tx, mut rx) = tokio::sync::watch::channel(String::from("Ctrl+Alt+Q"));
        // No hotkey change: the retry interval elapses and we re-register.
        assert!(!wait_for_retry(&mut rx, Duration::from_millis(10)).await);
    }

    #[tokio::test]
    async fn wait_for_retry_stops_on_shutdown() {
        let (tx, mut rx) = tokio::sync::watch::channel(String::from("Ctrl+Alt+Q"));
        drop(tx);
        assert!(wait_for_retry(&mut rx, Duration::from_secs(60)).await);
    }

    #[tokio::test]
    async fn wait_for_stable_hotkey_re_registers_when_listener_ends() {
        let (_tx, mut rx) = tokio::sync::watch::channel(String::from("Ctrl+Alt+Q"));
        let mut listener = tokio::spawn(async {});
        // The registration task ended (portal restart / bus drop): re-register.
        assert!(!wait_for_stable_hotkey(&mut rx, &mut listener).await);
    }

    #[tokio::test]
    async fn wait_for_stable_hotkey_stops_on_shutdown() {
        let (tx, mut rx) = tokio::sync::watch::channel(String::from("Ctrl+Alt+Q"));
        drop(tx);
        let mut listener = tokio::spawn(async {
            tokio::time::sleep(Duration::from_secs(60)).await;
        });
        assert!(wait_for_stable_hotkey(&mut rx, &mut listener).await);
    }
}
