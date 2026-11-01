//! Versioned migration driver tests: fresh databases get the baseline schema,
//! legacy unversioned databases are introspected and migrated forward, and
//! already-current databases open as a no-op.

use cliptoo_core::db::DbPool;

/// The v1 schema: the original shipped `clips` table (before `IsDeadhead` (v2)
/// and `IsFileUri` (v3) existed) plus its FTS index, which v1 always created
/// and the v5 migration writes to.
const V1_CREATE_CLIPS: &str = "
CREATE TABLE clips (
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
CREATE VIRTUAL TABLE clips_fts USING fts5(
    Content,
    Tags,
    content='clips',
    content_rowid='Id'
);
";

fn db_path(name: &str) -> std::path::PathBuf {
    let p = std::env::temp_dir().join(format!("{name}_{}", std::process::id()));
    let _ = std::fs::remove_file(&p);
    let _ = std::fs::remove_file(p.with_extension("wal"));
    let _ = std::fs::remove_file(p.with_extension("shm"));
    p
}

fn cleanup(p: &std::path::Path) {
    let _ = std::fs::remove_file(p);
    let _ = std::fs::remove_file(p.with_extension("wal"));
    let _ = std::fs::remove_file(p.with_extension("shm"));
}

fn user_version(path: &std::path::Path) -> i64 {
    let conn = rusqlite::Connection::open(path).unwrap();
    conn.pragma_query_value(None, "user_version", |row| row.get::<_, i64>(0))
        .unwrap()
}

fn columns_of(path: &std::path::Path, table: &str) -> Vec<String> {
    let conn = rusqlite::Connection::open(path).unwrap();
    let mut stmt = conn
        .prepare(&format!("PRAGMA table_info({table})"))
        .unwrap();
    stmt.query_map([], |row| row.get::<_, String>(1))
        .unwrap()
        .collect::<std::result::Result<_, _>>()
        .unwrap()
}

fn has_browse_index(path: &std::path::Path) -> bool {
    let conn = rusqlite::Connection::open(path).unwrap();
    let mut stmt = conn
        .prepare("SELECT name FROM sqlite_master WHERE type = 'index' AND tbl_name = 'clips'")
        .unwrap();
    stmt.query_map([], |row| row.get::<_, String>(0))
        .unwrap()
        .collect::<std::result::Result<Vec<_>, _>>()
        .unwrap()
        .iter()
        .any(|n| n == "idx_clips_ts_all")
}

fn fts_secure_delete(path: &std::path::Path) -> bool {
    let conn = rusqlite::Connection::open(path).unwrap();
    conn.query_row(
        "SELECT CAST(v AS INTEGER) FROM clips_fts_config WHERE k = 'secure-delete'",
        [],
        |row| row.get::<_, i64>(0),
    )
    .map(|v| v == 1)
    .unwrap_or(false)
}

/// A brand-new database is created at the current schema version: the migrated
/// columns are present, the browse-path index exists, and it is stamped.
/// Reopening it is a no-op.
#[test]
fn fresh_database_gets_baseline_at_current_version() {
    let p = db_path("cliptoo_mig_fresh");
    let db = DbPool::open(&p).unwrap();
    drop(db);

    let cols = columns_of(&p, "clips");
    assert!(cols.contains(&"IsDeadhead".to_string()));
    assert!(cols.contains(&"IsFileUri".to_string()));
    assert!(
        cols.contains(&"IsPinned".to_string()),
        "v6 pin column in baseline"
    );
    assert!(has_browse_index(&p), "v4 browse-path index in baseline");
    assert!(fts_secure_delete(&p), "v5 secure-delete in baseline");
    assert_eq!(
        user_version(&p),
        cliptoo_core::db::schema::SCHEMA_VERSION as i64
    );

    // Reopening an already-current database is a no-op.
    let db = DbPool::open(&p).unwrap();
    drop(db);
    assert_eq!(
        user_version(&p),
        cliptoo_core::db::schema::SCHEMA_VERSION as i64
    );

    cleanup(&p);
}

/// A legacy unversioned database from before `IsDeadhead` / `IsFileUri` is
/// introspected, stamped at v1, and migrated: both columns are added and its
/// existing rows survive.
#[test]
fn migrates_legacy_v1_database() {
    let p = db_path("cliptoo_mig_v1");
    {
        let conn = rusqlite::Connection::open(&p).unwrap();
        conn.execute_batch(V1_CREATE_CLIPS).unwrap();
        conn.execute(
            "INSERT INTO clips (Content, ContentHash, ClipType, Timestamp)
             VALUES (?1, ?2, 'text', '2020-01-01 00:00:00')",
            rusqlite::params!["legacy row", "legacyhash"],
        )
        .unwrap();
    }

    let db = DbPool::open(&p).unwrap();
    drop(db);

    let cols = columns_of(&p, "clips");
    assert!(
        cols.contains(&"IsDeadhead".to_string()),
        "v2 migration applied"
    );
    assert!(
        cols.contains(&"IsFileUri".to_string()),
        "v3 migration applied"
    );
    assert!(
        cols.contains(&"IsPinned".to_string()),
        "v6 migration applied"
    );
    assert!(has_browse_index(&p), "v4 migration applied");
    assert!(fts_secure_delete(&p), "v5 migration applied");
    assert_eq!(
        user_version(&p),
        cliptoo_core::db::schema::SCHEMA_VERSION as i64
    );

    // Existing data survives the migration.
    let conn = rusqlite::Connection::open(&p).unwrap();
    let content: String = conn
        .query_row("SELECT Content FROM clips WHERE Id = 1", [], |row| {
            row.get(0)
        })
        .unwrap();
    assert_eq!(content, "legacy row");

    cleanup(&p);
}

/// A legacy database with `IsDeadhead` but not `IsFileUri` runs only the v3
/// migration; the v2 column is left untouched.
#[test]
fn migrates_legacy_v2_database() {
    let p = db_path("cliptoo_mig_v2");
    {
        let conn = rusqlite::Connection::open(&p).unwrap();
        conn.execute_batch(V1_CREATE_CLIPS).unwrap();
        conn.execute_batch("ALTER TABLE clips ADD COLUMN IsDeadhead INTEGER NOT NULL DEFAULT 0")
            .unwrap();
    }

    let db = DbPool::open(&p).unwrap();
    drop(db);

    let cols = columns_of(&p, "clips");
    assert!(cols.contains(&"IsDeadhead".to_string()));
    assert!(cols.contains(&"IsFileUri".to_string()));
    assert_eq!(
        user_version(&p),
        cliptoo_core::db::schema::SCHEMA_VERSION as i64
    );

    cleanup(&p);
}

/// A v5 database's bottom-pin sentinel timestamps are backfilled into
/// `IsPinned`, so pins recorded before the column existed still survive
/// retention.
#[test]
fn migrates_v5_sentinel_pins_into_is_pinned() {
    let p = db_path("cliptoo_mig_v5pin");
    {
        let conn = rusqlite::Connection::open(&p).unwrap();
        conn.execute_batch(V1_CREATE_CLIPS).unwrap();
        conn.execute_batch(
            "ALTER TABLE clips ADD COLUMN IsDeadhead INTEGER NOT NULL DEFAULT 0;
             ALTER TABLE clips ADD COLUMN IsFileUri INTEGER NOT NULL DEFAULT 0;",
        )
        .unwrap();
        conn.execute_batch(
            "INSERT INTO clips (Content, ContentHash, ClipType, Timestamp) VALUES
                ('pinned', 'pin1', 'text', '1970-01-01 00:00:00.999999999999990'),
                ('legacy pin', 'pin2', 'text', '1970-01-01 00:00:00'),
                ('normal', 'n1', 'text', '2026-01-01 00:00:00.000000');",
        )
        .unwrap();
        conn.pragma_update(None, "user_version", 5).unwrap();
    }

    let db = DbPool::open(&p).unwrap();
    drop(db);

    let conn = rusqlite::Connection::open(&p).unwrap();
    let mut stmt = conn
        .prepare("SELECT ContentHash, IsPinned FROM clips ORDER BY Id")
        .unwrap();
    let pinned: Vec<(String, i64)> = stmt
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
        .unwrap()
        .collect::<std::result::Result<_, _>>()
        .unwrap();
    assert_eq!(
        pinned,
        vec![
            ("pin1".to_string(), 1),
            ("pin2".to_string(), 1),
            ("n1".to_string(), 0),
        ],
        "only sentinel timestamps are backfilled as pins"
    );
    assert_eq!(
        user_version(&p),
        cliptoo_core::db::schema::SCHEMA_VERSION as i64
    );

    cleanup(&p);
}

/// A legacy unversioned database that already has both migrated columns is
/// stamped at v3 and only the v4 browse-path index migration runs.
#[test]
fn stamps_legacy_database_without_reapplying_migrations() {
    let p = db_path("cliptoo_mig_v3");
    {
        let conn = rusqlite::Connection::open(&p).unwrap();
        conn.execute_batch(V1_CREATE_CLIPS).unwrap();
        conn.execute_batch(
            "ALTER TABLE clips ADD COLUMN IsDeadhead INTEGER NOT NULL DEFAULT 0;
             ALTER TABLE clips ADD COLUMN IsFileUri INTEGER NOT NULL DEFAULT 0;",
        )
        .unwrap();
    }

    let db = DbPool::open(&p).unwrap();
    drop(db);

    assert!(has_browse_index(&p), "v4 index migration applied");
    assert_eq!(
        user_version(&p),
        cliptoo_core::db::schema::SCHEMA_VERSION as i64
    );

    cleanup(&p);
}
