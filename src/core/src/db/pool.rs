// Single-connection pool. rusqlite::Connection is Send but not Sync, so we wrap it
// in tokio::sync::Mutex (which is Send+Sync when T: Send) for safe cross-task access.
// Sufficient for a desktop clipboard daemon.

use crate::db::schema;
use anyhow::Result;
use rusqlite::Connection;
use std::path::Path;

pub struct DbPool {
    // Single connection guarded by a tokio mutex; sufficient for a desktop clipboard daemon.
    // tokio::sync::Mutex is Send+Sync when T: Send (rusqlite::Connection is Send but not Sync).
    conn: tokio::sync::Mutex<Connection>,
}

impl DbPool {
    pub fn open(path: &Path) -> Result<Self> {
        let conn = Connection::open(path)?;

        // PRAGMAs must run outside any transaction.
        conn.execute_batch(schema::PRAGMA_WAL)?;
        conn.execute_batch(schema::PRAGMA_FOREIGN_KEYS)?;

        // Apply all idempotent schema DDL atomically.  If the process crashes
        // mid-setup, SQLite rolls back the transaction and the next open
        // retries the full block cleanly.
        conn.execute_batch(&format!(
            "BEGIN;\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}\nCOMMIT;",
            schema::CREATE_CLIPS,
            schema::CREATE_CLIPS_FTS,
            schema::CREATE_STATS,
            schema::SEED_STATS,
            schema::TRIGGER_FTS_INSERT,
            schema::TRIGGER_FTS_DELETE,
            schema::TRIGGER_FTS_UPDATE,
            schema::CREATE_INDEX_CLIPS_TS,
        ))?;

        // Column migrations for databases created by older versions.
        // `CREATE TABLE IF NOT EXISTS` never alters existing tables, so the
        // new column must be added explicitly when it is missing.
        if !conn
            .prepare("PRAGMA table_info(clips)")?
            .query_map([], |row| row.get::<_, String>(1))?
            .collect::<Result<Vec<_>, _>>()?
            .iter()
            .any(|name| name == "IsFileUri")
        {
            conn.execute_batch(schema::MIGRATE_ADD_IS_FILE_URI)?;
        }

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
