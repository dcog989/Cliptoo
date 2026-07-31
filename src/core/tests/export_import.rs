use cliptoo_core::content::classifier::ContentProcessor;
use cliptoo_core::db::DbPool;
use std::sync::Arc;

async fn insert_text(db: &Arc<DbPool>, content: &str, clip_type: &str) {
    let c = ContentProcessor::process(content).unwrap();
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
        )
    })
    .await
    .unwrap();
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
