use cliptoo_core::content::classifier::ContentProcessor;
use cliptoo_core::db::DbPool;

/// Insert a clip via `insert_or_bump` using the real `ContentProcessor` for
/// derived fields (preview, flags, size). `hash` is stored verbatim — pass the
/// same hash again to bump the existing row instead of inserting a new one.
pub async fn insert_clip(db: &DbPool, content: &str, hash: &str, clip_type: &str) {
    let c = ContentProcessor::process(content, false).unwrap();
    db.with(|conn| {
        cliptoo_core::db::queries::insert_or_bump(
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
