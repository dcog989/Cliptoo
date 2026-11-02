//! Shared, lazily-initialized D-Bus session connection.
//!
//! `zbus::Connection::session()` opens a fresh socket and runs the SASL
//! handshake on every call — it is not cached internally by zbus. Several
//! call sites need a session-bus connection (`theme::detect_system_dark`,
//! `theme::detect_system_accent`, and the settings theme preview), so
//! establishing it once and reusing the same `Connection` (which is cheaply
//! `Clone`, backed by an `Arc` internally) avoids paying that setup cost
//! repeatedly. The active-window tracker owns its own connection instead:
//! it needs a well-known name and an object server.

use std::sync::Mutex;
use zbus::Connection;

static SESSION_CONNECTION: Mutex<Option<Connection>> = Mutex::new(None);

/// The shared session-bus connection, connecting on first use only, and
/// reconnecting after the bus it was connected to has gone away (a
/// dbus-broker restart would otherwise leave every later call on a dead
/// socket).
pub async fn session() -> zbus::Result<Connection> {
    // Blocking lock is fine: it is only held for a field check or a clone,
    // never across an await.
    let cached = SESSION_CONNECTION
        .lock()
        .expect("dbus connection mutex poisoned")
        .as_ref()
        .filter(|conn| !conn.is_closed())
        .cloned();
    if let Some(conn) = cached {
        return Ok(conn);
    }

    let conn = Connection::session().await?;
    *SESSION_CONNECTION
        .lock()
        .expect("dbus connection mutex poisoned") = Some(conn.clone());
    Ok(conn)
}
