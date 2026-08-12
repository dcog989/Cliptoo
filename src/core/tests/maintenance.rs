mod common;

use cliptoo_core::content::classifier::ContentProcessor;
use cliptoo_core::db::DbPool;
use cliptoo_core::maintenance;
use std::sync::Arc;

#[tokio::test]
async fn reclassify_only_updates_rows_whose_classification_changed() {
    let dir = std::env::temp_dir().join(format!("cliptoo_reclassify_{}", std::process::id()));
    let db = Arc::new(DbPool::open(&dir).unwrap());

    // URL stored with the wrong type — ContentProcessor classifies it as "link".
    common::insert_clip(&db, "https://example.com", "urlhash", "text").await;
    // Correctly classified text clip — ContentProcessor also yields "text".
    common::insert_clip(&db, "hello world", "texthash", "text").await;
    // Freshly copied clip whose stored content was trimmed (leading whitespace).
    // WasTrimmed/HasLeadingWhitespace cannot be re-derived from the stored
    // trimmed content, so reclassify must leave it alone — type is unchanged.
    common::insert_clip(&db, "  indented line  ", "indenthash", "text").await;

    let first = maintenance::reclassify_all(&db).await.unwrap();
    assert_eq!(first, 1, "only the misclassified URL should be updated");

    // A second pass must not touch anything: nothing changed.
    let second = maintenance::reclassify_all(&db).await.unwrap();
    assert_eq!(second, 0, "no rows change on a second pass");

    let types: Vec<String> = db
        .with(|conn| {
            let mut stmt = conn.prepare_cached("SELECT ClipType FROM clips ORDER BY Id")?;
            let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
            let mut v = Vec::new();
            for r in rows {
                v.push(r?);
            }
            Ok(v)
        })
        .await
        .unwrap();
    assert_eq!(
        types,
        vec!["link".to_string(), "text".to_string(), "text".to_string()]
    );

    // The trimmed clip's WasTrimmed / HasLeadingWhitespace flags are preserved.
    let trim: (bool, bool) = db
        .with(|conn| {
            let (w, h): (i32, i32) = conn.query_row(
                "SELECT WasTrimmed, HasLeadingWhitespace FROM clips WHERE Id = 3",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )?;
            Ok((w != 0, h != 0))
        })
        .await
        .unwrap();
    assert_eq!(trim, (true, true));

    let _ = std::fs::remove_file(&dir);
    let _ = std::fs::remove_file(dir.with_extension("wal"));
    let _ = std::fs::remove_file(dir.with_extension("shm"));
}

/// Image clips store an internal path under `images_dir` as their Content
/// (the listener's image branch), which the classifier would misread as a
/// plain text path and reclassify to `file_path`. reclassify must leave
/// `file_image` rows untouched — their type is not re-derivable.
#[tokio::test]
async fn reclassify_leaves_image_clips_untouched() {
    let dir = std::env::temp_dir().join(format!("cliptoo_reclassify_img_{}", std::process::id()));
    clean_up(&dir);
    let db = Arc::new(DbPool::open(&dir).unwrap());

    common::insert_clip(
        &db,
        "/home/user/.local/share/Cliptoo/images/abcdef1234567890.png",
        "imghash",
        "file_image",
    )
    .await;

    let n = maintenance::reclassify_all(&db).await.unwrap();
    assert_eq!(n, 0, "file_image clips are never reclassified");

    let clip_type: String = db
        .with(|conn| {
            let t: String =
                conn.query_row("SELECT ClipType FROM clips WHERE Id = 1", [], |row| {
                    row.get(0)
                })?;
            Ok(t)
        })
        .await
        .unwrap();
    assert_eq!(clip_type, "file_image");

    clean_up(&dir);
}

fn clean_up(dir: &std::path::Path) {
    let _ = std::fs::remove_file(dir);
    let _ = std::fs::remove_file(dir.with_extension("wal"));
    let _ = std::fs::remove_file(dir.with_extension("shm"));
}

/// "Move to bottom" is a keep signal: retention must sweep a genuinely old
/// clip while sparing the pinned one, even though the pinned clip's epoch
/// sentinel timestamp is older than any age cutoff. Both the age and count
/// clauses are exercised.
#[tokio::test]
async fn retention_spares_bottom_pinned_clips() {
    let dir = std::env::temp_dir().join(format!("cliptoo_retpin_{}", std::process::id()));
    clean_up(&dir);
    let db = DbPool::open(&dir).unwrap();

    common::insert_clip(&db, "one", "hash_one", "text").await;
    common::insert_clip(&db, "two", "hash_two", "text").await;
    common::insert_clip(&db, "three", "hash_three", "text").await;

    // Pin id 1 to the bottom; age id 3 back to 2020 so both retention clauses
    // would otherwise target it.
    db.with(|conn| cliptoo_core::db::queries::bump_to_bottom(conn, 1))
        .await
        .unwrap();
    db.with(|conn| {
        conn.execute(
            "UPDATE clips SET Timestamp = '2020-01-01 00:00:00' WHERE Id = 3",
            [],
        )?;
        Ok(())
    })
    .await
    .unwrap();

    let cfg = cliptoo_core::maintenance::RetentionConfig {
        max_clips: 1,
        max_age_days: 1,
    };
    let deleted = db
        .with(|conn| cliptoo_core::maintenance::retention(conn, &cfg))
        .await
        .unwrap();
    assert_eq!(
        deleted, 1,
        "only the genuinely old, non-pinned clip is swept"
    );

    let ids: Vec<i64> = db
        .with(|conn| {
            let mut stmt = conn.prepare_cached("SELECT Id FROM clips ORDER BY Id")?;
            let rows = stmt.query_map([], |row| row.get::<_, i64>(0))?;
            let mut v = Vec::new();
            for r in rows {
                v.push(r?);
            }
            Ok(v)
        })
        .await
        .unwrap();
    assert_eq!(
        ids,
        vec![1, 2],
        "the pinned clip and the newest clip survive"
    );

    clean_up(&dir);
}

/// Bottom pins written before the distinct-timestamp change (plain
/// `1970-01-01 00:00:00`, no Id suffix) must also be exempt — the predicate
/// matches by timestamp prefix, not by the newer sentinel format.
#[tokio::test]
async fn retention_spares_legacy_plain_epoch_bottom_pins() {
    let dir = std::env::temp_dir().join(format!("cliptoo_retpinlegacy_{}", std::process::id()));
    clean_up(&dir);
    let db = DbPool::open(&dir).unwrap();

    common::insert_clip(&db, "one", "hash_one", "text").await;
    // Simulate a pre-fix bottom pin: plain epoch timestamp, no Id suffix.
    db.with(|conn| {
        conn.execute(
            "UPDATE clips SET Timestamp = '1970-01-01 00:00:00' WHERE Id = 1",
            [],
        )?;
        Ok(())
    })
    .await
    .unwrap();

    let cfg = cliptoo_core::maintenance::RetentionConfig {
        max_clips: 1,
        max_age_days: 30,
    };
    let deleted = db
        .with(|conn| cliptoo_core::maintenance::retention(conn, &cfg))
        .await
        .unwrap();
    assert_eq!(deleted, 0, "the legacy-format bottom pin is spared");

    let count: i64 = db
        .with(|conn| {
            let n = conn.query_row("SELECT COUNT(*) FROM clips", [], |row| row.get(0))?;
            Ok(n)
        })
        .await
        .unwrap();
    assert_eq!(count, 1);

    clean_up(&dir);
}

/// The async delete path checks file existence outside the DB lock and removes
/// only clips whose paths are genuinely gone — a live file clip must survive.
#[tokio::test]
async fn delete_deadheads_removes_only_missing_paths() {
    let dir = std::env::temp_dir().join(format!("cliptoo_deldead_{}", std::process::id()));
    clean_up(&dir);
    let db = Arc::new(DbPool::open(&dir).unwrap());

    // A real file clip — must survive.
    let file_dir =
        std::env::temp_dir().join(format!("cliptoo_deldead_file_{}", std::process::id()));
    std::fs::create_dir_all(&file_dir).unwrap();
    let live = file_dir.join("live.txt");
    std::fs::write(&live, b"x").unwrap();
    common::insert_clip(&db, live.to_str().unwrap(), "dead_live", "file_generic").await;

    // A clip whose path no longer exists — must be removed.
    let gone = file_dir.join("gone.txt");
    common::insert_clip(&db, gone.to_str().unwrap(), "dead_gone", "file_generic").await;

    let deleted = cliptoo_core::maintenance::delete_deadheads(&db)
        .await
        .unwrap();
    assert_eq!(deleted, 1);

    let ids: Vec<i64> = db
        .with(|conn| {
            let mut stmt = conn.prepare_cached("SELECT Id FROM clips ORDER BY Id")?;
            let rows = stmt.query_map([], |row| row.get::<_, i64>(0))?;
            let mut v = Vec::new();
            for r in rows {
                v.push(r?);
            }
            Ok(v)
        })
        .await
        .unwrap();
    assert_eq!(ids.len(), 1, "only the missing-path clip is removed");

    clean_up(&dir);
    let _ = std::fs::remove_dir_all(&file_dir);
}

/// The scheduled task must read fresh retention parameters from the watch
/// channel on each pass — a setting change applies without a restart.
#[tokio::test]
async fn scheduler_applies_updated_retention_config() {
    let dir = std::env::temp_dir().join(format!("cliptoo_sched_{}", std::process::id()));
    clean_up(&dir);
    let db = Arc::new(DbPool::open(&dir).unwrap());

    let thumbs = std::env::temp_dir().join(format!("cliptoo_sched_thumbs_{}", std::process::id()));
    let favs = std::env::temp_dir().join(format!("cliptoo_sched_favs_{}", std::process::id()));
    std::fs::create_dir_all(&thumbs).unwrap();
    std::fs::create_dir_all(&favs).unwrap();

    for i in 0..5 {
        common::insert_clip(&db, &format!("clip {i}"), &format!("schash{i}"), "text").await;
    }

    let (tx, rx) = tokio::sync::watch::channel(cliptoo_core::maintenance::RetentionConfig {
        max_clips: 1000,
        max_age_days: 0,
    });
    cliptoo_core::maintenance::spawn_scheduler(db.clone(), thumbs.clone(), favs.clone(), rx, 1);

    // First pass runs at the initial config (keeps everything); then publish a
    // lower cap before the second pass and wait for it to prune.
    tokio::time::sleep(std::time::Duration::from_millis(1500)).await;
    tx.send(cliptoo_core::maintenance::RetentionConfig {
        max_clips: 2,
        max_age_days: 0,
    })
    .unwrap();

    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    loop {
        let n = db
            .with(cliptoo_core::db::queries::count_clips)
            .await
            .unwrap();
        if n <= 2 {
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "scheduler did not apply the updated max_clips"
        );
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }

    clean_up(&dir);
    let _ = std::fs::remove_dir_all(&thumbs);
    let _ = std::fs::remove_dir_all(&favs);
}

/// A stored file clip whose path no longer exists keeps its specific type: the
/// classifier recomputes it as `file_generic` because it can only see current
/// disk state, but the type reflects what was copied — deadhead already flags
/// the missing path, so reclassify must not degrade it.
#[tokio::test]
async fn reclassify_keeps_specific_type_for_missing_file_clips() {
    let dir = std::env::temp_dir().join(format!("cliptoo_reclassify_gone_{}", std::process::id()));
    clean_up(&dir);
    let db = Arc::new(DbPool::open(&dir).unwrap());

    // A copied-file clip (IsFileUri=1) stored as file_archive; the file is gone.
    let missing = std::env::temp_dir().join(format!(
        "cliptoo_reclassify_gone_file_{}",
        std::process::id()
    ));
    let missing_str = missing.to_str().unwrap().to_string();
    let c = ContentProcessor::process(&missing_str, true).unwrap();
    db.with(|conn| {
        cliptoo_core::db::queries::insert_or_bump(
            conn,
            &missing_str,
            &c.preview_content,
            "gonehash",
            "file_archive",
            None,
            c.was_trimmed,
            c.has_leading_whitespace,
            c.is_multiline,
            c.size_in_bytes,
            true,
        )
    })
    .await
    .unwrap();

    let n = maintenance::reclassify_all(&db).await.unwrap();
    assert_eq!(n, 0, "missing-path file clip keeps its stored type");

    let clip_type: String = db
        .with(|conn| {
            let t: String =
                conn.query_row("SELECT ClipType FROM clips WHERE Id = 1", [], |row| {
                    row.get(0)
                })?;
            Ok(t)
        })
        .await
        .unwrap();
    assert_eq!(clip_type, "file_archive");

    clean_up(&dir);
}

/// `mark_deadheads` returns the number of rows *newly* marked — a clip already
/// flagged in a previous pass must not be counted again.
#[tokio::test]
async fn mark_deadheads_counts_only_newly_marked_rows() {
    let dir = std::env::temp_dir().join(format!("cliptoo_markdead_{}", std::process::id()));
    clean_up(&dir);
    let db = Arc::new(DbPool::open(&dir).unwrap());

    // Two file clips whose paths do not exist on disk.
    let gone_a = std::env::temp_dir().join(format!("cliptoo_markdead_a_{}", std::process::id()));
    let gone_b = std::env::temp_dir().join(format!("cliptoo_markdead_b_{}", std::process::id()));
    common::insert_clip(&db, gone_a.to_str().unwrap(), "markdead_1", "file_generic").await;
    common::insert_clip(&db, gone_b.to_str().unwrap(), "markdead_2", "file_generic").await;

    let first = maintenance::mark_deadheads(&db).await.unwrap();
    assert_eq!(first, 2);

    let second = maintenance::mark_deadheads(&db).await.unwrap();
    assert_eq!(second, 0, "already-marked rows are not counted again");

    clean_up(&dir);
}

/// Count-based retention keeps the newest `max_clips` non-bookmarked clips and
/// drops the rest, even when they are all well inside the age window.
#[tokio::test]
async fn retention_enforces_max_clips_count() {
    let dir = std::env::temp_dir().join(format!("cliptoo_retcount_{}", std::process::id()));
    clean_up(&dir);
    let db = DbPool::open(&dir).unwrap();

    for i in 0..5 {
        common::insert_clip(&db, &format!("clip {i}"), &format!("rcount{i}"), "text").await;
    }

    let cfg = cliptoo_core::maintenance::RetentionConfig {
        max_clips: 2,
        max_age_days: 0,
    };
    let deleted = db
        .with(|conn| cliptoo_core::maintenance::retention(conn, &cfg))
        .await
        .unwrap();
    assert_eq!(deleted, 3);

    let n: i64 = db
        .with(|conn| {
            let n: i64 = conn.query_row("SELECT COUNT(*) FROM clips", [], |row| row.get(0))?;
            Ok(n)
        })
        .await
        .unwrap();
    assert_eq!(n, 2);

    clean_up(&dir);
}

/// `clear_history` removes non-bookmarked clips but spares bookmarks.
#[tokio::test]
async fn clear_history_keeps_bookmarked_clips() {
    let dir = std::env::temp_dir().join(format!("cliptoo_clearhist_{}", std::process::id()));
    clean_up(&dir);
    let db = DbPool::open(&dir).unwrap();

    common::insert_clip(&db, "regular", "chist_1", "text").await;
    common::insert_clip(&db, "kept", "chist_2", "text").await;

    let kept_id = db
        .with(|conn| cliptoo_core::db::queries::search_clips(conn, "kept", "all", 10, 0, None))
        .await
        .unwrap()[0]
        .id;
    db.with(|conn| cliptoo_core::db::queries::set_bookmarked(conn, kept_id, true))
        .await
        .unwrap();

    let deleted = db
        .with(cliptoo_core::maintenance::clear_history)
        .await
        .unwrap();
    assert_eq!(deleted, 1);

    let n: i64 = db
        .with(|conn| {
            let n: i64 = conn.query_row("SELECT COUNT(*) FROM clips", [], |row| row.get(0))?;
            Ok(n)
        })
        .await
        .unwrap();
    assert_eq!(n, 1);

    clean_up(&dir);
}

/// `prune_cache` must survive a DB row whose ContentHash is not valid UTF-8 on
/// the 16-byte boundary (corrupt/legacy data written before import validation
/// existed). The prefix derivation is a byte slice, so without an
/// `is_char_boundary` guard this would panic inside the scheduled maintenance
/// task; the orphan thumbnail must still be pruned.
#[tokio::test]
async fn prune_cache_tolerates_multibyte_content_hash() {
    let dir = std::env::temp_dir().join(format!("cliptoo_prunehash_{}", std::process::id()));
    clean_up(&dir);
    let db = Arc::new(DbPool::open(&dir).unwrap());

    // 15 ASCII chars + a 4-byte emoji: byte 16 lands inside the emoji, so a
    // bare `h[..16]` would panic. The row is skipped as a known prefix.
    common::insert_clip(&db, "clip", "123456789012345\u{1F600}", "text").await;

    let thumbs =
        std::env::temp_dir().join(format!("cliptoo_prunehash_thumbs_{}", std::process::id()));
    let favicons =
        std::env::temp_dir().join(format!("cliptoo_prunehash_favs_{}", std::process::id()));
    std::fs::create_dir_all(&thumbs).unwrap();
    std::fs::create_dir_all(&favicons).unwrap();
    // A genuine orphan (16-char hex prefix, no matching DB row) must be pruned.
    std::fs::write(thumbs.join("deadbeefdeadbeef.webp"), b"x").unwrap();

    let pruned = maintenance::prune_cache(&db, &thumbs, &favicons)
        .await
        .unwrap();
    assert_eq!(
        pruned, 1,
        "orphan thumbnail pruned without panicking on the malformed hash"
    );

    clean_up(&dir);
    let _ = std::fs::remove_dir_all(&thumbs);
    let _ = std::fs::remove_dir_all(&favicons);
}
