use anyhow::Result;
use rusqlite::{Connection, params};

pub const KEY_UNIQUE_CLIPS_EVER: &str = "UniqueClipsEver";
pub const KEY_PASTE_COUNT: &str = "PasteCount";
pub const KEY_LAST_CLEANUP: &str = "LastCleanupTimestamp";
pub const KEY_CREATION: &str = "CreationTimestamp";

pub fn get_stat(conn: &Connection, key: &str) -> Result<Option<String>> {
    let result = conn.query_row(
        "SELECT Value FROM stats WHERE Key = ?1",
        params![key],
        |row| row.get(0),
    );
    match result {
        Ok(v) => Ok(Some(v)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(e.into()),
    }
}

pub fn set_stat(conn: &Connection, key: &str, value: &str) -> Result<()> {
    conn.execute(
        "INSERT OR REPLACE INTO stats (Key, Value) VALUES (?1, ?2)",
        params![key, value],
    )?;
    Ok(())
}

/// Atomically increment an integer stat.
///
pub fn increment_stat(conn: &Connection, key: &str) -> Result<()> {
    conn.execute(
        "INSERT INTO stats (Key, Value) VALUES (?1, 1)
         ON CONFLICT(Key) DO UPDATE SET Value = CAST(CAST(Value AS INTEGER) + 1 AS TEXT)",
        params![key],
    )?;
    Ok(())
}
