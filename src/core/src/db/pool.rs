// Single-connection pool. rusqlite::Connection is Send but not Sync, so we wrap it
// in tokio::sync::Mutex (which is Send+Sync when T: Send) for safe cross-task access.
// Sufficient for a desktop clipboard daemon.

use crate::db::schema;
use anyhow::Result;
use rusqlite::Connection;
use std::path::Path;

/// rusqlite's default prepared-statement cache capacity is 16. `search_clips`
/// alone builds 50+ distinct SQL strings (4 query shapes × ~14 filter
/// variants), plus a handful more from maintenance/export — well past the
/// default, which would silently evict and recompile statements on the
/// keystroke hot path instead of reusing them.
const STATEMENT_CACHE_CAPACITY: usize = 128;

pub struct DbPool {
    // Single connection guarded by a tokio mutex; sufficient for a desktop clipboard daemon.
    // tokio::sync::Mutex is Send+Sync when T: Send (rusqlite::Connection is Send but not Sync).
    conn: tokio::sync::Mutex<Connection>,
}

impl DbPool {
    pub fn open(path: &Path) -> Result<Self> {
        let conn = Connection::open(path)?;
        conn.set_prepared_statement_cache_capacity(STATEMENT_CACHE_CAPACITY);

        // PRAGMAs must run outside any transaction.
        conn.execute_batch(schema::PRAGMA_WAL)?;
        conn.execute_batch(schema::PRAGMA_FOREIGN_KEYS)?;

        // Versioned schema + migrations (see `schema::migrate`); no-ops when
        // already current.
        schema::migrate(&conn)?;

        Ok(Self {
            conn: tokio::sync::Mutex::new(conn),
        })
    }

    pub async fn with<F, R>(&self, f: F) -> Result<R>
    where
        F: FnOnce(&Connection) -> Result<R>,
    {
        let conn = self.conn.lock().await;
        f(&conn)
    }
}
