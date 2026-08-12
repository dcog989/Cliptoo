mod common;

use cliptoo_core::content::classifier::ContentProcessor;
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

fn clean_up(dir: &std::path::Path) {
    let _ = std::fs::remove_file(dir);
    let _ = std::fs::remove_file(dir.with_extension("wal"));
    let _ = std::fs::remove_file(dir.with_extension("shm"));
}

/// Imported clips with foreign (ISO-8601 `T`-separated) timestamps must be
/// normalised to the canonical space-separated UTC form. The ordering and
/// retention queries compare `Timestamp` strings lexicographically against
/// space-separated cutoffs, so an ISO `T` (0x54 > 0x20) would sort after the
/// cutoff on the same date and evade the age-based retention sweep.
#[tokio::test]
async fn import_normalizes_foreign_timestamps_to_canonical_form() {
    let dir = std::env::temp_dir().join(format!("cliptoo_impts_{}", std::process::id()));
    clean_up(&dir);
    let db = Arc::new(DbPool::open(&dir).unwrap());

    let json = r#"[
        {"id": 1, "content": "ancient clip", "preview_content": "ancient clip",
         "content_hash": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
         "clip_type": "text", "source_app": null,
         "timestamp": "2020-01-01T00:00:00", "is_bookmarked": false,
         "was_trimmed": false, "has_leading_whitespace": false,
         "is_multiline": false, "size_in_bytes": 12, "paste_count": 0, "tags": null}
    ]"#;
    let inserted = db
        .with(|conn| cliptoo_core::export::import_json(conn, json.as_bytes()))
        .await
        .unwrap();
    assert_eq!(inserted, 1);

    let ts: String = db
        .with(|conn| {
            let t = conn.query_row("SELECT Timestamp FROM clips WHERE Id = 1", [], |row| {
                row.get(0)
            })?;
            Ok(t)
        })
        .await
        .unwrap();
    assert_eq!(
        ts, "2020-01-01 00:00:00.000000",
        "foreign timestamp must be normalised to canonical UTC form"
    );

    // End to end: the canonical value is a real, old time, so age-based
    // retention sweeps it.
    let cfg = cliptoo_core::maintenance::RetentionConfig {
        max_clips: 0,
        max_age_days: 30,
    };
    let deleted = db
        .with(|conn| cliptoo_core::maintenance::retention(conn, &cfg))
        .await
        .unwrap();
    assert_eq!(
        deleted, 1,
        "ancient imported clip is swept by age retention"
    );

    clean_up(&dir);
}

/// An unparseable imported timestamp falls back to the current time, so the
/// row still imports while keeping the canonical shape (garbage in the
/// `Timestamp` column would poison ordering and retention comparisons).
#[tokio::test]
async fn import_falls_back_to_current_time_for_unparseable_timestamps() {
    let dir = std::env::temp_dir().join(format!("cliptoo_imptsbad_{}", std::process::id()));
    clean_up(&dir);
    let db = Arc::new(DbPool::open(&dir).unwrap());

    let json = r#"[
        {"id": 1, "content": "weird clip", "preview_content": "weird clip",
         "content_hash": "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
         "clip_type": "text", "source_app": null,
         "timestamp": "yesterday-ish", "is_bookmarked": false,
         "was_trimmed": false, "has_leading_whitespace": false,
         "is_multiline": false, "size_in_bytes": 10, "paste_count": 0, "tags": null}
    ]"#;
    let inserted = db
        .with(|conn| cliptoo_core::export::import_json(conn, json.as_bytes()))
        .await
        .unwrap();
    assert_eq!(inserted, 1);

    let ts: String = db
        .with(|conn| {
            let t = conn.query_row("SELECT Timestamp FROM clips WHERE Id = 1", [], |row| {
                row.get(0)
            })?;
            Ok(t)
        })
        .await
        .unwrap();
    // Canonical µs shape: `YYYY-MM-DD HH:MM:SS.ffffff` (26 chars, space at 10).
    assert_eq!(ts.len(), 26, "canonical timestamp, got {ts:?}");
    assert_eq!(ts.as_bytes()[10], b' ', "space separator, got {ts:?}");

    clean_up(&dir);
}

/// Rows whose `content_hash` is not a canonical 64-char lowercase hex digest
/// are rejected (skipped), so a corrupt or foreign hash can never reach the
/// DB where `prune_cache` byte-slices its first 16 bytes into a filename key.
#[tokio::test]
async fn import_skips_rows_with_invalid_content_hash() {
    let dir = std::env::temp_dir().join(format!("cliptoo_imphash_{}", std::process::id()));
    clean_up(&dir);
    let db = Arc::new(DbPool::open(&dir).unwrap());

    let json = r#"[
        {"id": 1, "content": "good clip", "preview_content": "good clip",
         "content_hash": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
         "clip_type": "text", "source_app": null, "timestamp": "2020-01-01 00:00:00",
         "is_bookmarked": false, "was_trimmed": false, "has_leading_whitespace": false,
         "is_multiline": false, "size_in_bytes": 9, "paste_count": 0, "tags": null},
        {"id": 2, "content": "short hash", "preview_content": "short hash",
         "content_hash": "nothex", "clip_type": "text", "source_app": null,
         "timestamp": "2020-01-02 00:00:00", "is_bookmarked": false,
         "was_trimmed": false, "has_leading_whitespace": false, "is_multiline": false,
         "size_in_bytes": 10, "paste_count": 0, "tags": null},
        {"id": 3, "content": "utf8 hash", "preview_content": "utf8 hash",
         "content_hash": "123456789012345😀", "clip_type": "text", "source_app": null,
         "timestamp": "2020-01-03 00:00:00", "is_bookmarked": false,
         "was_trimmed": false, "has_leading_whitespace": false, "is_multiline": false,
         "size_in_bytes": 9, "paste_count": 0, "tags": null},
        {"id": 4, "content": "uppercase hex", "preview_content": "uppercase hex",
         "content_hash": "ABCDEFABCDEFABCDEFABCDEFABCDEFABCDEFABCDEFABCDEFABCDEFABCDEFABCD",
         "clip_type": "text", "source_app": null, "timestamp": "2020-01-04 00:00:00",
         "is_bookmarked": false, "was_trimmed": false, "has_leading_whitespace": false,
         "is_multiline": false, "size_in_bytes": 12, "paste_count": 0, "tags": null}
    ]"#;
    let inserted = db
        .with(|conn| cliptoo_core::export::import_json(conn, json.as_bytes()))
        .await
        .unwrap();
    assert_eq!(
        inserted, 1,
        "only the valid-hash row is imported; malformed rows are skipped"
    );

    let hashes: Vec<String> = db
        .with(|conn| {
            let mut stmt = conn.prepare_cached("SELECT ContentHash FROM clips")?;
            let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
            let mut v = Vec::new();
            for r in rows {
                v.push(r?);
            }
            Ok(v)
        })
        .await
        .unwrap();
    assert_eq!(hashes.len(), 1);
    assert!(
        cliptoo_core::export::is_valid_content_hash(&hashes[0]),
        "stored hash is canonical"
    );

    clean_up(&dir);
}

/// A copied-file clip (IsFileUri=1) must keep that flag through an
/// export/import round-trip. reclassify re-runs the classifier with the stored
/// IsFileUri flag, so without it the clip would be read as path-looking text
/// and silently downgraded from a file_* type to file_path.
#[tokio::test]
async fn import_preserves_is_file_uri_across_reclassify() {
    let dir = std::env::temp_dir().join(format!("cliptoo_isuri_{}", std::process::id()));
    clean_up(&dir);
    let db1 = Arc::new(DbPool::open(&dir).unwrap());

    // A copied file: Content is a real path, IsFileUri=1, ClipType taken from
    // the classifier so reclassify agrees with the stored type. The payload
    // lives in its own directory — `dir` is a SQLite file, not a folder.
    let file_dir = std::env::temp_dir().join(format!("cliptoo_isuri_file_{}", std::process::id()));
    std::fs::create_dir_all(&file_dir).unwrap();
    let file_path = file_dir.join("payload.txt");
    std::fs::write(&file_path, b"x").unwrap();
    let path_str = file_path.to_str().unwrap().to_string();

    let c = ContentProcessor::process(&path_str, true).unwrap();
    db1.with(|conn| {
        cliptoo_core::db::queries::insert_or_bump(
            conn,
            &path_str,
            &c.preview_content,
            &c.content_hash,
            c.clip_type.as_str(),
            None,
            c.was_trimmed,
            c.has_leading_whitespace,
            c.is_multiline,
            c.size_in_bytes,
            true, // is_file_uri — copied as a file
        )
    })
    .await
    .unwrap();
    let stored_type = c.clip_type.as_str().to_string();

    let out = dir.with_extension("json");
    cliptoo_core::export::export_to_file(&db1, &out)
        .await
        .unwrap();

    let db2 = Arc::new(DbPool::open(&dir.with_extension("2")).unwrap());
    let inserted = cliptoo_core::export::import_from_file(&db2, &out)
        .await
        .unwrap();
    assert_eq!(inserted, 1);

    let n = cliptoo_core::maintenance::reclassify_all(&db2)
        .await
        .unwrap();
    assert_eq!(n, 0, "file clip must not be reclassified after import");

    let (clip_type, is_file_uri): (String, i32) = db2
        .with(|conn| {
            let t = conn.query_row(
                "SELECT ClipType, IsFileUri FROM clips WHERE Id = 1",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )?;
            Ok(t)
        })
        .await
        .unwrap();
    assert_eq!(clip_type, stored_type);
    assert_eq!(is_file_uri, 1);

    clean_up(&dir);
    clean_up(&dir.with_extension("2"));
    let _ = std::fs::remove_file(&out);
    let _ = std::fs::remove_dir_all(&file_dir);
}
