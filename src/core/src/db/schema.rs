//! SQL DDL for Cliptoo's SQLite schema and the versioned migration driver
//! ([`migrate`]) that applies it. `DbPool::open` runs `migrate` on every
//! launch: fresh databases get the baseline schema, older databases are
//! migrated up to [`SCHEMA_VERSION`] under `PRAGMA user_version` control.

use anyhow::Result;
use rusqlite::Connection;

pub const CREATE_CLIPS: &str = "
CREATE TABLE IF NOT EXISTS clips (
    Id                    INTEGER PRIMARY KEY AUTOINCREMENT,
    Content               TEXT,
    PreviewContent        TEXT,
    ContentHash           TEXT NOT NULL UNIQUE,
    ClipType              TEXT NOT NULL,
    SourceApp             TEXT,
    Timestamp             TEXT NOT NULL,
    IsBookmarked          INTEGER NOT NULL DEFAULT 0,
    WasTrimmed            INTEGER NOT NULL DEFAULT 0,
    HasLeadingWhitespace  INTEGER NOT NULL DEFAULT 0,
    IsMultiline           INTEGER NOT NULL DEFAULT 0,
    IsDeadhead            INTEGER NOT NULL DEFAULT 0,
    IsFileUri             INTEGER NOT NULL DEFAULT 0,
    SizeInBytes           INTEGER NOT NULL DEFAULT 0,
    PasteCount            INTEGER NOT NULL DEFAULT 0,
    Tags                  TEXT
);
";

pub const CREATE_CLIPS_FTS: &str = "
CREATE VIRTUAL TABLE IF NOT EXISTS clips_fts USING fts5(
    Content,
    Tags,
    content='clips',
    content_rowid='Id'
);
";

pub const CREATE_STATS: &str = "
CREATE TABLE IF NOT EXISTS stats (
    Key   TEXT PRIMARY KEY,
    Value TEXT
);
";

pub const SEED_STATS: &str = "
INSERT OR IGNORE INTO stats (Key, Value) VALUES
    ('UniqueClipsEver',     '0'),
    ('PasteCount',          '0'),
    ('LastCleanupTimestamp', ''),
    ('CreationTimestamp',   datetime('now'));
";

pub const TRIGGER_FTS_INSERT: &str = "
CREATE TRIGGER IF NOT EXISTS clips_fts_insert AFTER INSERT ON clips BEGIN
    INSERT INTO clips_fts (rowid, Content, Tags) VALUES (new.Id, new.Content, new.Tags);
END;
";

pub const TRIGGER_FTS_DELETE: &str = "
CREATE TRIGGER IF NOT EXISTS clips_fts_delete AFTER DELETE ON clips BEGIN
    INSERT INTO clips_fts (clips_fts, rowid, Content, Tags) VALUES ('delete', old.Id, old.Content, old.Tags);
END;
";

pub const TRIGGER_FTS_UPDATE: &str = "
CREATE TRIGGER IF NOT EXISTS clips_fts_update AFTER UPDATE OF Content, Tags ON clips BEGIN
    INSERT INTO clips_fts (clips_fts, rowid, Content, Tags) VALUES ('delete', old.Id, old.Content, old.Tags);
    INSERT INTO clips_fts (rowid, Content, Tags) VALUES (new.Id, new.Content, new.Tags);
END;
";

pub const PRAGMA_WAL: &str = "PRAGMA journal_mode = WAL;";
pub const PRAGMA_FOREIGN_KEYS: &str = "PRAGMA foreign_keys = ON;";
// NOTE: WAL mode.
// `PRAGMA journal_mode = WAL` is a no-op if another process already has the
// database open in a different mode; SQLite will silently keep the existing
// journal mode and `execute_batch` will succeed without error.  For this
// single-process desktop daemon that is fine, but any external debug tool
// opening the database will suppress WAL on that launch.

pub const CREATE_INDEX_CLIPS_TS: &str =
    "CREATE INDEX IF NOT EXISTS idx_clips_ts ON clips(IsBookmarked, Timestamp DESC);";

/// v2 migration: add the `IsDeadhead` column for databases created before the
/// deadhead feature shipped. Existing rows default to live (0). Gated by
/// `user_version` in [`migrate`] — never a per-launch guard — so the plain
/// `ALTER TABLE` is safe even though SQLite rejects adding a column that
/// already exists.
pub const MIGRATE_ADD_IS_DEADHEAD: &str =
    "ALTER TABLE clips ADD COLUMN IsDeadhead INTEGER NOT NULL DEFAULT 0";

/// v3 migration: add the `IsFileUri` column for databases created before it
/// existed, with the text-origin default so pre-existing rows keep behaving as
/// text. Gated by `user_version` in [`migrate`] — never a per-launch guard.
pub const MIGRATE_ADD_IS_FILE_URI: &str =
    "ALTER TABLE clips ADD COLUMN IsFileUri INTEGER NOT NULL DEFAULT 0";

/// Current schema version. Bump when the schema changes and append a matching
/// entry to `MIGRATIONS`; never edit or reorder a migration that has shipped.
pub const SCHEMA_VERSION: u32 = 3;

/// Ordered schema migrations. Each entry raises the schema to `target_version`
/// and runs exactly once, inside its own transaction that also stamps
/// `user_version` — a crash mid-migration rolls back both the schema change
/// and the stamp, so the next open retries cleanly.
const MIGRATIONS: &[(u32, &str)] = &[(2, MIGRATE_ADD_IS_DEADHEAD), (3, MIGRATE_ADD_IS_FILE_URI)];

/// Bring `conn` up to [`SCHEMA_VERSION`]. No-ops when already current.
///
/// * **Fresh database** (no `clips` table): the full baseline schema is built
///   atomically in one transaction and stamped at `SCHEMA_VERSION`.
/// * **Versioned database**: pending migrations run in order, each in its own
///   transaction.
/// * **Legacy unversioned database**: introspected once to determine the
///   highest migration already applied (by column presence), stamped, then
///   migrated forward like a versioned database. The baseline tables are
///   assumed present — every shipped version created them on first open, so
///   only columns are ever missing.
pub fn migrate(conn: &Connection) -> Result<()> {
    let mut version = user_version(conn)?;

    if version >= SCHEMA_VERSION {
        return Ok(());
    }

    if version == 0 {
        if !table_exists(conn, "clips")? {
            return apply_baseline(conn);
        }
        version = legacy_version(conn)?;
        set_user_version(conn, version)?;
    }

    for (target, sql) in MIGRATIONS {
        if version < *target {
            let tx = conn.unchecked_transaction()?;
            tx.execute_batch(sql)?;
            tx.pragma_update(None, "user_version", *target)?;
            tx.commit()?;
            version = *target;
        }
    }
    Ok(())
}

/// Create the full baseline schema for a brand-new database and stamp it at
/// `SCHEMA_VERSION`, all in one transaction. A crash mid-setup leaves
/// `user_version` at 0 and the next open retries the whole block.
fn apply_baseline(conn: &Connection) -> Result<()> {
    let tx = conn.unchecked_transaction()?;
    tx.execute_batch(&format!(
        "{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}",
        CREATE_CLIPS,
        CREATE_CLIPS_FTS,
        CREATE_STATS,
        SEED_STATS,
        TRIGGER_FTS_INSERT,
        TRIGGER_FTS_DELETE,
        TRIGGER_FTS_UPDATE,
        CREATE_INDEX_CLIPS_TS,
    ))?;
    tx.pragma_update(None, "user_version", SCHEMA_VERSION)?;
    tx.commit()?;
    Ok(())
}

fn user_version(conn: &Connection) -> Result<u32> {
    let v: i64 = conn.pragma_query_value(None, "user_version", |row| row.get(0))?;
    Ok(v as u32)
}

fn set_user_version(conn: &Connection, v: u32) -> Result<()> {
    conn.pragma_update(None, "user_version", v)?;
    Ok(())
}

fn table_exists(conn: &Connection, name: &str) -> Result<bool> {
    let n: i64 = conn.query_row(
        "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = ?1",
        [name],
        |row| row.get(0),
    )?;
    Ok(n > 0)
}

fn column_exists(conn: &Connection, table: &str, column: &str) -> Result<bool> {
    let mut stmt = conn.prepare(&format!("PRAGMA table_info({table})"))?;
    let columns: Vec<String> = stmt
        .query_map([], |row| row.get::<_, String>(1))?
        .collect::<std::result::Result<_, _>>()?;
    Ok(columns.iter().any(|c| c == column))
}

/// Infer the version of a legacy database written before `user_version`
/// stamping existed. Advances past every migration whose column is already
/// present, so an unversioned database lands exactly where its schema puts it
/// and only genuinely missing changes run.
fn legacy_version(conn: &Connection) -> Result<u32> {
    let mut version = 1;
    if column_exists(conn, "clips", "IsDeadhead")? {
        version = 2;
    }
    if column_exists(conn, "clips", "IsFileUri")? {
        version = 3;
    }
    Ok(version)
}

// NOTE: FTS column name coupling.
// `clips_fts` is declared with `content='clips'`, binding it to the `clips`
// table.  The column names used in the FTS virtual table definition (`Content`,
// `Tags`) and in every trigger below must exactly match the `clips` column
// names — SQLite FTS5 is case-sensitive on some builds.  If either column is
// ever renamed in `CREATE_CLIPS`, the FTS table definition, all three triggers,
// and any `INSERT INTO clips_fts(clips_fts) VALUES('rebuild')` call must be
// updated atomically in the same migration — i.e. in a single entry of
// `MIGRATIONS`, so the coupled change lands in one transaction gated by
// `user_version`.
//
// NOTE: FTS divergence risk.
// Because `clips_fts` is an *external content* table, any write to `clips`
// that bypasses the three triggers above (e.g. a direct `UPDATE clips SET
// Content = ...` outside of the trigger columns, or a bulk import) will leave
// the FTS index stale without any error.  To detect divergence early, the
// scheduled maintenance pass in `maintenance.rs` should run:
//   INSERT INTO clips_fts(clips_fts) VALUES('integrity-check');
// which returns an error string if the FTS shadow tables are inconsistent with
// `clips`.  A full rebuild (slow but safe) is:
//   INSERT INTO clips_fts(clips_fts) VALUES('rebuild');
