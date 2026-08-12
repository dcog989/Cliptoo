mod common;

use cliptoo_core::db::DbPool;
use std::sync::Arc;

fn clean_up(dir: &std::path::Path) {
    let _ = std::fs::remove_file(dir);
    let _ = std::fs::remove_file(dir.with_extension("wal"));
    let _ = std::fs::remove_file(dir.with_extension("shm"));
}

async fn temp_db(name: &str) -> (std::path::PathBuf, Arc<DbPool>) {
    let dir = std::env::temp_dir().join(format!("cliptoo_{name}_{}", std::process::id()));
    clean_up(&dir);
    let db = Arc::new(DbPool::open(&dir).unwrap());
    (dir, db)
}

/// `update_clip_metadata` refreshes the file-derived size/multiline flags of
/// an existing clip without touching its content or preview (the edit path
/// for `file_text` clips, whose Content stays the file path).
#[tokio::test]
async fn update_clip_metadata_refreshes_size_and_multiline() {
    let (dir, db) = temp_db("metadata_refresh").await;
    common::insert_clip(&db, "/tmp/doc.txt", "hash-a", "file_text").await;

    let id = db
        .with(|conn| {
            cliptoo_core::db::queries::search_clips(conn, "", "all", 10, 0, None)
                .map(|clips| clips[0].id)
        })
        .await
        .unwrap();

    db.with(|conn| cliptoo_core::db::queries::update_clip_metadata(conn, id, 8192, true))
        .await
        .unwrap();

    let after = db
        .with(|conn| {
            cliptoo_core::db::queries::search_clips(conn, "", "all", 10, 0, None)
                .map(|clips| (clips[0].size_in_bytes, clips[0].is_multiline))
        })
        .await
        .unwrap();
    assert_eq!(after, (8192, true));

    // Content (the path) is untouched.
    let content = db
        .with(|conn| cliptoo_core::db::queries::get_clip_content(conn, id))
        .await
        .unwrap();
    assert_eq!(content, "/tmp/doc.txt");

    clean_up(&dir);
}
