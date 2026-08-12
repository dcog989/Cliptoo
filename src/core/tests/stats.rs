mod common;

use cliptoo_core::db::DbPool;
use cliptoo_core::db::queries;
use cliptoo_core::stats;

#[tokio::test]
async fn count_clips_counts_stored_rows() {
    let dir = std::env::temp_dir().join(format!("cliptoo_stats_{}", std::process::id()));
    let db = DbPool::open(&dir).unwrap();

    common::insert_clip(&db, "first", "h1", "text").await;
    common::insert_clip(&db, "second", "h2", "text").await;
    common::insert_clip(&db, "third", "h3", "text").await;

    let total = db.with(queries::count_clips).await.unwrap();
    assert_eq!(total, 3);

    // Re-copying an existing hash bumps it to the top, not a new row.
    common::insert_clip(&db, "second", "h2", "text").await;
    let total = db.with(queries::count_clips).await.unwrap();
    assert_eq!(total, 3);

    let _ = std::fs::remove_file(&dir);
    let _ = std::fs::remove_file(dir.with_extension("wal"));
    let _ = std::fs::remove_file(dir.with_extension("shm"));
}

#[tokio::test]
async fn record_paste_bumps_global_paste_counter() {
    let dir = std::env::temp_dir().join(format!("cliptoo_paste_{}", std::process::id()));
    let db = DbPool::open(&dir).unwrap();

    common::insert_clip(&db, "hello", "h1", "text").await;
    let id = db
        .with(|conn| queries::search_clips(conn, "", "all", 10, 0, None))
        .await
        .unwrap()[0]
        .id;
    db.with(|conn| queries::record_paste(conn, id))
        .await
        .unwrap();
    db.with(|conn| queries::record_paste(conn, id))
        .await
        .unwrap();

    let pastes = db
        .with(|conn| stats::get_stat(conn, stats::KEY_PASTE_COUNT))
        .await
        .unwrap();
    assert_eq!(pastes.unwrap(), "2");

    // The per-clip paste count is also incremented.
    let count: i64 = db
        .with(|conn| {
            let n: i64 =
                conn.query_row("SELECT PasteCount FROM clips WHERE Id = ?1", [id], |row| {
                    row.get(0)
                })?;
            Ok(n)
        })
        .await
        .unwrap();
    assert_eq!(count, 2);

    let _ = std::fs::remove_file(&dir);
    let _ = std::fs::remove_file(dir.with_extension("wal"));
    let _ = std::fs::remove_file(dir.with_extension("shm"));
}

#[tokio::test]
async fn increment_stat_upserts_and_accumulates() {
    let dir = std::env::temp_dir().join(format!("cliptoo_statinc_{}", std::process::id()));
    let db = DbPool::open(&dir).unwrap();

    let key = "TestCounter";
    db.with(|conn| stats::increment_stat(conn, key))
        .await
        .unwrap();
    db.with(|conn| stats::increment_stat(conn, key))
        .await
        .unwrap();
    db.with(|conn| stats::increment_stat(conn, key))
        .await
        .unwrap();

    let value = db.with(|conn| stats::get_stat(conn, key)).await.unwrap();
    assert_eq!(value.unwrap(), "3");

    // set_stat replaces the accumulated value.
    db.with(|conn| stats::set_stat(conn, key, "42"))
        .await
        .unwrap();
    let value = db.with(|conn| stats::get_stat(conn, key)).await.unwrap();
    assert_eq!(value.unwrap(), "42");

    // A missing key reads as None.
    let missing = db
        .with(|conn| stats::get_stat(conn, "NoSuchKey"))
        .await
        .unwrap();
    assert_eq!(missing, None);

    let _ = std::fs::remove_file(&dir);
    let _ = std::fs::remove_file(dir.with_extension("wal"));
    let _ = std::fs::remove_file(dir.with_extension("shm"));
}
