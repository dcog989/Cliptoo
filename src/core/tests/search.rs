//! Search-input handling: the query string comes straight from the search box,
//! so FTS5 syntax must never turn user text into an error.

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

/// A query with no terms (spaces only) used to build `MATCH ''`, which is an
/// SQLite error — the UI then silently kept the previous result list.
#[tokio::test]
async fn whitespace_only_query_returns_browse_list() {
    let (dir, db) = temp_db("search_ws").await;
    common::insert_clip(&db, "hello world", "hash_hello", "text").await;

    for query in ["", " ", "   \t "] {
        let clips = db
            .with(|conn| cliptoo_core::db::queries::search_clips(conn, query, "all", 10, 0, None))
            .await
            .unwrap();
        assert_eq!(clips.len(), 1, "for query {query:?}");
    }

    clean_up(&dir);
}

/// FTS5 operators and quotes typed into the search box are literal text, not
/// syntax.
#[tokio::test]
async fn fts_special_characters_do_not_error() {
    let (dir, db) = temp_db("search_special").await;
    common::insert_clip(&db, "say \"hi\" now", "hash_say", "text").await;

    for query in ["\"", "\"\"", "-", "*", "(", "NEAR(a b)", "\"hi\""] {
        let result = db
            .with(|conn| cliptoo_core::db::queries::search_clips(conn, query, "all", 10, 0, None))
            .await;
        assert!(result.is_ok(), "query {query:?} must not error: {result:?}");
    }

    clean_up(&dir);
}

/// Every clip type the UI can filter by maps to its stored value; an unknown
/// filter string is "no filter" (the whole list), never a silent partial match.
#[tokio::test]
async fn type_filter_matches_only_that_type() {
    let (dir, db) = temp_db("search_filter").await;
    common::insert_clip(&db, "hello world", "hash_hello", "text").await;
    common::insert_clip(&db, "#ff0000", "hash_color", "color").await;

    let color = db
        .with(|conn| cliptoo_core::db::queries::search_clips(conn, "", "color", 10, 0, None))
        .await
        .unwrap();
    assert_eq!(color.len(), 1);
    assert_eq!(color[0].clip_type.as_str(), "color");

    let text = db
        .with(|conn| cliptoo_core::db::queries::search_clips(conn, "", "text", 10, 0, None))
        .await
        .unwrap();
    assert_eq!(text.len(), 1);
    assert_eq!(text[0].clip_type.as_str(), "text");

    let unknown = db
        .with(|conn| cliptoo_core::db::queries::search_clips(conn, "", "not_a_type", 10, 0, None))
        .await
        .unwrap();
    assert_eq!(unknown.len(), 2, "an unknown filter must not filter");

    clean_up(&dir);
}

/// The snippet markers must reach the caller: the UI splits on exactly these
/// strings, and a printable marker could collide with clip text.
#[tokio::test]
async fn content_snippet_carries_the_highlight_sentinels() {
    let (dir, db) = temp_db("search_hl").await;
    common::insert_clip(&db, "hello world", "hash_hello", "text").await;

    let clips = db
        .with(|conn| cliptoo_core::db::queries::search_clips(conn, "hello", "all", 10, 0, None))
        .await
        .unwrap();
    let context = clips[0].match_context.as_deref().unwrap_or("");
    assert!(
        context.contains(cliptoo_core::db::queries::FTS_HL_OPEN)
            && context.contains(cliptoo_core::db::queries::FTS_HL_CLOSE),
        "snippet must carry the highlight sentinels: {context:?}"
    );

    clean_up(&dir);
}

/// The tag prefix routes the query to the Tags column; `##` alone lists every
/// tagged clip, and `##work` matches tags by prefix.
#[tokio::test]
async fn tag_prefix_search_lists_and_matches_tags() {
    let (dir, db) = temp_db("search_tags").await;
    common::insert_clip(&db, "first clip", "hash_one", "text").await;
    common::insert_clip(&db, "second clip", "hash_two", "text").await;

    let id: i64 = db
        .with(|conn| {
            Ok(conn.query_row(
                "SELECT Id FROM clips WHERE ContentHash = 'hash_one'",
                [],
                |row| row.get(0),
            )?)
        })
        .await
        .unwrap();
    db.with(|conn| cliptoo_core::db::queries::update_tags(conn, id, "work project"))
        .await
        .unwrap();

    let tagged = db
        .with(|conn| cliptoo_core::db::queries::search_clips(conn, "##", "all", 10, 0, Some("##")))
        .await
        .unwrap();
    assert_eq!(tagged.len(), 1, "only the tagged clip is listed");

    let by_tag = db
        .with(|conn| {
            cliptoo_core::db::queries::search_clips(conn, "##work", "all", 10, 0, Some("##"))
        })
        .await
        .unwrap();
    assert_eq!(by_tag.len(), 1);
    assert_eq!(by_tag[0].id, id);

    // A background refresh has no settings handle and passes `None`; it must
    // keep the tag semantics instead of falling back to a content search. A
    // third clip whose *content* contains "work" catches that fallback.
    common::insert_clip(&db, "workshop notes", "hash_three", "text").await;
    let by_tag_default = db
        .with(|conn| cliptoo_core::db::queries::search_clips(conn, "##work", "all", 10, 0, None))
        .await
        .unwrap();
    assert_eq!(by_tag_default.len(), 1);
    assert_eq!(by_tag_default[0].id, id);

    clean_up(&dir);
}
