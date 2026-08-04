mod common;

use cliptoo_core::db::DbPool;
use std::sync::Arc;

/// Insert a text clip, deriving its hash from the content.
async fn insert_text(db: &Arc<DbPool>, content: &str, clip_type: &str) {
    common::insert_clip(db, content, &format!("testhash_{content}"), clip_type).await;
}

#[tokio::test]
async fn export_bookmarks_only() {
    let dir = std::env::temp_dir().join(format!("cliptoo_bm_{}", std::process::id()));
    let db = Arc::new(DbPool::open(&dir).unwrap());

    insert_text(&db, "plain clip", "text").await;
    insert_text(&db, "fav clip", "text").await;

    let fav_id = db
        .with(|conn| cliptoo_core::db::queries::search_clips(conn, "fav", "all", 10, 0, None))
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

    assert!(
        text.contains("fav clip"),
        "bookmarked clip must be exported"
    );
    assert!(
        !text.contains("plain clip"),
        "non-bookmarked clip must be skipped"
    );
    assert_eq!(text.matches("\"content\": \"fav clip\"").count(), 1);

    let _ = std::fs::remove_file(&out);
    let _ = std::fs::remove_file(&dir);
    let _ = std::fs::remove_file(dir.with_extension("wal"));
    let _ = std::fs::remove_file(dir.with_extension("shm"));
}

/// Bookmarked clips must be weighted above regular clips in full-text search:
/// a bookmark ranks ahead of a non-bookmark even when its raw match is slightly
/// worse (the rank bonus outweighs the gap, so ties and close matches both win).
#[tokio::test]
async fn search_ranks_bookmarks_first() {
    // Distinct content + hashes (insert_or_bump dedupes on hash), so the FTS
    // ranks differ; the bookmark rank bonus must still lift it to the top.
    let dir = std::env::temp_dir().join(format!("cliptoo_bmrank_{}", std::process::id()));
    let db = Arc::new(DbPool::open(&dir).unwrap());
    let clips = [
        ("the quick brown fox", "hash_plain_best"),
        ("quick fox runs fast", "hash_bm"),
        ("a quick fox", "hash_plain_2"),
    ];

    for (content, hash) in clips {
        db.with(|conn| {
            let c = cliptoo_core::content::classifier::ContentProcessor::process(content, false)
                .unwrap();
            cliptoo_core::db::queries::insert_or_bump(
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
                false,
            )
        })
        .await
        .unwrap();
    }

    // Identify the bookmark row by hash, then bookmark it.
    let all = db
        .with(|conn| cliptoo_core::db::queries::search_clips(conn, "quick fox", "all", 10, 0, None))
        .await
        .unwrap();
    let bookmarked = all
        .iter()
        .find(|c| c.content_hash == "hash_bm")
        .expect("bookmark clip present")
        .id;
    db.with(|conn| cliptoo_core::db::queries::set_bookmarked(conn, bookmarked, true))
        .await
        .unwrap();

    let results = db
        .with(|conn| cliptoo_core::db::queries::search_clips(conn, "quick fox", "all", 10, 0, None))
        .await
        .unwrap();
    assert!(
        results.len() >= 3,
        "expected all three clips to match the FTS query"
    );
    assert_eq!(
        results[0].id, bookmarked,
        "bookmarked clip must be weighted above non-bookmarked matches"
    );

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
