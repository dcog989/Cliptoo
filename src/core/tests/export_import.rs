use cliptoo_core::content::classifier::ContentProcessor;
use cliptoo_core::db::DbPool;
use std::sync::Arc;

async fn insert_text(db: &Arc<DbPool>, content: &str, clip_type: &str) {
    let c = ContentProcessor::process(content, false).unwrap();
    db.with(|conn| {
        cliptoo_core::db::queries::insert_or_bump(
            conn,
            content,
            &c.preview_content,
            &format!("testhash_{content}"),
            clip_type,
            None,
            c.was_trimmed,
            c.has_leading_whitespace,
            c.is_multiline,
            c.size_in_bytes,
            false,
        )
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn export_bookmarks_only() {
    let dir = std::env::temp_dir().join(format!("cliptoo_bm_{}", std::process::id()));
    let db = Arc::new(DbPool::open(&dir).unwrap());

    insert_text(&db, "plain clip", "hash_plain").await;
    insert_text(&db, "fav clip", "hash_fav").await;

    let fav_id = db
        .with(|conn| {
            cliptoo_core::db::queries::search_clips(conn, "fav", "all", 10, 0, None)
        })
        .await
        .unwrap()[0]
        .id;
    db.with(|conn| cliptoo_core::db::queries::set_bookmarked(conn, fav_id, true))
        .await
        .unwrap();

    let out = dir.with_extension("bm.json");
    cliptoo_core::export::export_bookmarked_to_file(&db, &out)
        .await
        .unwrap();
    let text = tokio::fs::read_to_string(&out).await.unwrap();

    assert!(text.contains("fav clip"), "bookmarked clip must be exported");
    assert!(!text.contains("plain clip"), "non-bookmarked clip must be skipped");
    assert_eq!(text.matches("\"content\": \"fav clip\"").count(), 1);

    let _ = std::fs::remove_file(&out);
    let _ = std::fs::remove_file(&dir);
    let _ = std::fs::remove_file(dir.with_extension("wal"));
    let _ = std::fs::remove_file(dir.with_extension("shm"));
}

#[tokio::test]
async fn export_import_roundtrip() {
    let dir = std::env::temp_dir().join(format!("cliptoo_ei_{}", std::process::id()));
    let db1 = Arc::new(DbPool::open(&dir).unwrap());

    insert_text(&db1, "hello world", "text").await;
    insert_text(&db1, "https://example.com", "link").await;

    let out = dir.with_extension("json");
    let n = cliptoo_core::export::export_to_file(&db1, &out)
        .await
        .unwrap();
    assert!(n > 0, "export produced bytes");

    let db2 = Arc::new(DbPool::open(&dir.with_extension("2")).unwrap());
    let inserted = cliptoo_core::export::import_from_file(&db2, &out)
        .await
        .unwrap();
    assert_eq!(inserted, 2);

    let _ = std::fs::remove_file(&out);
    let _ = std::fs::remove_file(&dir);
    let _ = std::fs::remove_file(dir.with_extension("2"));
}
