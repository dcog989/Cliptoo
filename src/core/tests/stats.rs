use cliptoo_core::content::classifier::ContentProcessor;
use cliptoo_core::db::DbPool;
use cliptoo_core::db::queries;
use cliptoo_core::stats;

async fn insert_text(db: &DbPool, content: &str, hash: &str) {
    let c = ContentProcessor::process(content).unwrap();
    db.with(|conn| {
        queries::insert_or_bump(
            conn,
            content,
            &c.preview_content,
            hash,
            "text",
            None,
            c.was_trimmed,
            c.has_leading_whitespace,
            c.is_multiline,
            c.size_in_bytes,
        )
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn count_clips_counts_stored_rows() {
    let dir = std::env::temp_dir().join(format!("cliptoo_stats_{}", std::process::id()));
    let db = DbPool::open(&dir).unwrap();

    insert_text(&db, "first", "h1").await;
    insert_text(&db, "second", "h2").await;
    insert_text(&db, "third", "h3").await;

    let total = db.with(queries::count_clips).await.unwrap();
    assert_eq!(total, 3);

    // Re-copying an existing hash bumps it to the top, not a new row.
    insert_text(&db, "second", "h2").await;
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

    insert_text(&db, "hello", "h1").await;

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
