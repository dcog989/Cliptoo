//! SQL DDL for Cliptoo's SQLite schema.
//! Run once on first launch via `apply_schema()`.

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

/// Migrations applied after `apply_schema()` for databases created before a
/// given column existed.  Each statement uses `ALTER TABLE … ADD COLUMN` which
/// is a no-op-safe pattern in SQLite: it will fail if the column already
/// exists, so callers must ignore `rusqlite::Error::SqliteFailure` with
/// `SQLITE_ERROR` (code 1) on these statements, or gate them on a
/// user_version PRAGMA.
pub const MIGRATE_ADD_HAS_LEADING_WHITESPACE: &str =
    "ALTER TABLE clips ADD COLUMN HasLeadingWhitespace INTEGER NOT NULL DEFAULT 0;";

pub const MIGRATE_ADD_IS_MULTILINE: &str =
    "ALTER TABLE clips ADD COLUMN IsMultiline INTEGER NOT NULL DEFAULT 0;";

pub const MIGRATE_ADD_IS_DEADHEAD: &str =
    "ALTER TABLE clips ADD COLUMN IsDeadhead INTEGER NOT NULL DEFAULT 0;";

pub const CREATE_INDEX_CLIPS_TS: &str =
    "CREATE INDEX IF NOT EXISTS idx_clips_ts ON clips(IsBookmarked, Timestamp DESC);";

// NOTE: FTS column name coupling.
// `clips_fts` is declared with `content='clips'`, binding it to the `clips`
// table.  The column names used in the FTS virtual table definition (`Content`,
// `Tags`) and in every trigger below must exactly match the `clips` column
// names — SQLite FTS5 is case-sensitive on some builds.  If either column is
// ever renamed in `CREATE_CLIPS`, the FTS table definition, all three triggers,
// and any `INSERT INTO clips_fts(clips_fts) VALUES('rebuild')` call must be
// updated atomically in the same migration.
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
