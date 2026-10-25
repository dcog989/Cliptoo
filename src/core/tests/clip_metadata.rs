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

/// Re-copying an existing clip refreshes the metadata derived from current disk
/// state instead of keeping the first copy's values, and clears a stale
/// deadhead flag — the caller just classified the content again, so the path
/// exists. Paired with the timestamp bump that brings the row to the top.
#[tokio::test]
async fn insert_or_bump_refreshes_stale_metadata() {
    let (dir, db) = temp_db("bump_refresh").await;

    db.with(|conn| {
        cliptoo_core::db::queries::insert_or_bump(
            conn,
            "/tmp/photo.png",
            "old preview",
            "hash-bump",
            "file_generic",
            Some("first-app"),
            false,
            false,
            false,
            42,
            false,
        )
    })
    .await
    .unwrap();

    let id: i64 = db
        .with(|conn| {
            conn.query_row(
                "SELECT Id FROM clips WHERE ContentHash = 'hash-bump'",
                [],
                |row| row.get(0),
            )
        })
        .await
        .unwrap();

    // A previous deadhead pass flagged the row while the path was missing.
    db.with(|conn| conn.execute("UPDATE clips SET IsDeadhead = 1 WHERE Id = ?1", [id]))
        .await
        .unwrap();

    let inserted = db
        .with(|conn| {
            cliptoo_core::db::queries::insert_or_bump(
                conn,
                "/tmp/photo.png",
                "new preview",
                "hash-bump",
                "file_image",
                Some("second-app"),
                false,
                false,
                true,
                4096,
                true,
            )
        })
        .await
        .unwrap();
    assert!(!inserted, "same hash must bump, not insert");

    let (clip, is_file_uri) = db
        .with(|conn| {
            let clip = cliptoo_core::db::queries::search_clips(conn, "", "all", 10, 0, None)?
                .into_iter()
                .next()
                .expect("bumped row is still listed");
            let is_file_uri: i32 = conn.query_row(
                "SELECT IsFileUri FROM clips WHERE Id = ?1",
                [clip.id],
                |row| row.get(0),
            )?;
            Ok((clip, is_file_uri))
        })
        .await
        .unwrap();

    assert_eq!(
        clip.clip_type,
        cliptoo_core::db::models::ClipType::FileImage
    );
    assert_eq!(clip.preview_content, "new preview");
    assert_eq!(clip.size_in_bytes, 4096);
    assert!(clip.is_multiline);
    assert!(!clip.is_deadhead);
    assert_eq!(clip.source_app.as_deref(), Some("second-app"));
    assert_eq!(is_file_uri, 1);

    clean_up(&dir);
}
