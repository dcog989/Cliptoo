use cliptoo_core::content::classifier::ContentProcessor;
use cliptoo_core::db::DbPool;
use cliptoo_core::db::queries;
use cliptoo_core::maintenance;

async fn insert_clip(db: &DbPool, content: &str, hash: &str, clip_type: &str) {
    let c = ContentProcessor::process(content, false).unwrap();
    db.with(|conn| {
        queries::insert_or_bump(
            conn,
            content,
            &c.preview_content,
            hash,
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
async fn reclassify_only_updates_rows_whose_classification_changed() {
    let dir = std::env::temp_dir().join(format!("cliptoo_reclassify_{}", std::process::id()));
    let db = DbPool::open(&dir).unwrap();

    // URL stored with the wrong type — ContentProcessor classifies it as "link".
    insert_clip(&db, "https://example.com", "urlhash", "text").await;
    // Correctly classified text clip — ContentProcessor also yields "text".
    insert_clip(&db, "hello world", "texthash", "text").await;
    // Freshly copied clip whose stored content was trimmed (leading whitespace).
    // WasTrimmed/HasLeadingWhitespace cannot be re-derived from the stored
    // trimmed content, so reclassify must leave it alone — type is unchanged.
    insert_clip(&db, "  indented line  ", "indenthash", "text").await;

    let first = db.with(maintenance::reclassify_all).await.unwrap();
    assert_eq!(first, 1, "only the misclassified URL should be updated");

    // A second pass must not touch anything: nothing changed.
    let second = db.with(maintenance::reclassify_all).await.unwrap();
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
