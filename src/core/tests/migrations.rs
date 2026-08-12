//! Versioned migration driver tests: fresh databases get the baseline schema,
//! legacy unversioned databases are introspected and migrated forward, and
//! already-current databases open as a no-op.

use cliptoo_core::db::DbPool;

/// The v1 `clips` table: the original shipped schema, before `IsDeadhead`
/// (v2) and `IsFileUri` (v3) existed.
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

/// A brand-new database is created at the current schema version: the migrated
/// columns are present and it is stamped. Reopening it is a no-op.
#[test]
fn fresh_database_gets_baseline_at_current_version() {
    let p = db_path("cliptoo_mig_fresh");
    let db = DbPool::open(&p).unwrap();
    drop(db);

    let cols = columns_of(&p, "clips");
    assert!(cols.contains(&"IsDeadhead".to_string()));
    assert!(cols.contains(&"IsFileUri".to_string()));
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

/// A legacy unversioned database that already has both migrated columns is
/// stamped at the current version without reapplying any migration.
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

    assert_eq!(
        user_version(&p),
        cliptoo_core::db::schema::SCHEMA_VERSION as i64
    );

    cleanup(&p);
}
