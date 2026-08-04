//! Shared, lazily-initialized D-Bus session connection.
//!
//! `zbus::Connection::session()` opens a fresh socket and runs the SASL
//! handshake on every call — it is not cached internally by zbus. Several
//! call sites need a session-bus connection (`source_app::detect_source_app`,
//! called on every clipboard capture; `theme::detect_system_dark` and
//! `theme::detect_system_accent`), so establishing it once and reusing the
//! same `Connection` (which is cheaply `Clone`, backed by an `Arc`
//! internally) avoids paying that setup cost repeatedly.

use tokio::sync::OnceCell;
use zbus::Connection;

static SESSION_CONNECTION: OnceCell<Connection> = OnceCell::const_new();

/// The shared session-bus connection, connecting on first use only.
pub async fn session() -> zbus::Result<Connection> {
    let conn = SESSION_CONNECTION
        .get_or_try_init(Connection::session)
        .await?;
    Ok(conn.clone())
}
