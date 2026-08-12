mod common;

use cliptoo_core::db::DbPool;
use std::sync::Arc;

/// The browse list order — `ORDER BY Timestamp DESC, Id DESC`.
async fn browse_order(db: &Arc<DbPool>) -> Vec<i64> {
    db.with(|conn| {
        cliptoo_core::db::queries::search_clips(conn, "", "all", 1000, 0, None)
            .map(|clips| clips.into_iter().map(|c| c.id).collect())
    })
    .await
    .unwrap()
}

async fn move_up(db: &Arc<DbPool>, id: i64) {
    db.with(|conn| cliptoo_core::db::queries::move_up_one(conn, id))
        .await
        .unwrap();
}

async fn move_down(db: &Arc<DbPool>, id: i64) {
    db.with(|conn| cliptoo_core::db::queries::move_down_one(conn, id))
        .await
        .unwrap();
}

fn clean_up(dir: &std::path::Path) {
    let _ = std::fs::remove_file(dir);
    let _ = std::fs::remove_file(dir.with_extension("wal"));
    let _ = std::fs::remove_file(dir.with_extension("shm"));
}

/// Fresh DB for a test, keyed by `name` so tests never share a file. The
/// order assertions rely on fresh AUTOINCREMENT ids (1, 2, 3), so any stale
/// database from a previously-interrupted run is removed first.
async fn temp_db(name: &str) -> (std::path::PathBuf, Arc<DbPool>) {
    let dir = std::env::temp_dir().join(format!("cliptoo_{name}_{}", std::process::id()));
    clean_up(&dir);
    let db = Arc::new(DbPool::open(&dir).unwrap());
    (dir, db)
}

/// Move-up/down must swap with the immediate neighbour exactly one position,
/// even for clips inserted back-to-back within the same second/millisecond —
/// write timestamps are strictly distinct, so the swap is never a no-op and
/// never skips a whole same-instant group.
#[tokio::test]
async fn move_up_and_down_swaps_with_immediate_neighbor() {
    let (dir, db) = temp_db("move").await;

    common::insert_clip(&db, "one", "hash_one", "text").await;
    common::insert_clip(&db, "two", "hash_two", "text").await;
    common::insert_clip(&db, "three", "hash_three", "text").await;

    // Newest first: ids 3, 2, 1.
    assert_eq!(browse_order(&db).await, vec![3, 2, 1]);

    // Move the oldest up one: [3, 1, 2].
    move_up(&db, 1).await;
    assert_eq!(browse_order(&db).await, vec![3, 1, 2]);

    // And up again: [1, 3, 2].
    move_up(&db, 1).await;
    assert_eq!(browse_order(&db).await, vec![1, 3, 2]);

    // Back down one: [3, 1, 2].
    move_down(&db, 1).await;
    assert_eq!(browse_order(&db).await, vec![3, 1, 2]);

    clean_up(&dir);
}

/// A clip already at the top cannot move up (and vice versa) — the call is a
/// no-op rather than an error.
#[tokio::test]
async fn moves_at_edges_are_noops() {
    let (dir, db) = temp_db("moveedge").await;

    common::insert_clip(&db, "one", "hash_one", "text").await;
    common::insert_clip(&db, "two", "hash_two", "text").await;

    move_up(&db, 2).await;
    assert_eq!(browse_order(&db).await, vec![2, 1], "top clip stays put");

    move_down(&db, 1).await;
    assert_eq!(browse_order(&db).await, vec![2, 1], "bottom clip stays put");

    clean_up(&dir);
}

/// Bottom-pinned clips get distinct epoch timestamps (keyed by Id), so moving
/// within the pinned group works instead of silently no-op'ing on a tie.
#[tokio::test]
async fn bottom_pinned_clips_stay_distinct_and_reorderable() {
    let (dir, db) = temp_db("pin").await;

    common::insert_clip(&db, "alpha", "hash_alpha", "text").await;
    common::insert_clip(&db, "beta", "hash_beta", "text").await;
    common::insert_clip(&db, "gamma", "hash_gamma", "text").await;

    db.with(|conn| cliptoo_core::db::queries::bump_to_bottom(conn, 2))
        .await
        .unwrap();
    db.with(|conn| cliptoo_core::db::queries::bump_to_bottom(conn, 1))
        .await
        .unwrap();

    // Bottom group (earlier-pinned id further down): [3, 2, 1].
    assert_eq!(browse_order(&db).await, vec![3, 2, 1]);

    // Move id 1 up past id 2 within the pinned group.
    move_up(&db, 1).await;
    assert_eq!(browse_order(&db).await, vec![3, 1, 2]);

    clean_up(&dir);
}
