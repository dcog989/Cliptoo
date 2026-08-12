mod common;

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
