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

/// A newly pinned clip goes to the absolute bottom whatever its `Id`, and
/// pinning it again after a move never reuses a pin suffix (two rows with one
/// `Timestamp` would make the next move a no-op).
#[tokio::test]
async fn move_to_bottom_puts_the_clip_last_regardless_of_id() {
    let (dir, db) = temp_db("pinorder").await;

    common::insert_clip(&db, "one", "hash_one", "text").await;
    common::insert_clip(&db, "two", "hash_two", "text").await;
    common::insert_clip(&db, "three", "hash_three", "text").await;

    let pin = |id: i64| {
        let db = db.clone();
        async move {
            db.with(|conn| cliptoo_core::db::queries::bump_to_bottom(conn, id))
                .await
                .unwrap();
        }
    };

    pin(1).await;
    assert_eq!(browse_order(&db).await, vec![3, 2, 1]);

    // Pinning id 2 after id 1 must put 2 below 1, not above it.
    pin(2).await;
    assert_eq!(browse_order(&db).await, vec![3, 1, 2]);

    // Move within the pinned group, then pin again: no duplicate timestamp, so
    // the following move still reorders.
    move_up(&db, 2).await;
    assert_eq!(browse_order(&db).await, vec![3, 2, 1]);
    pin(2).await;
    assert_eq!(browse_order(&db).await, vec![3, 1, 2]);
    move_up(&db, 2).await;
    assert_eq!(browse_order(&db).await, vec![3, 2, 1]);

    clean_up(&dir);
}

/// A pinned clip must not trade its bottom-pin sentinel (the retention keep
/// signal) with an ordinary neighbour. Moves across the boundary are refused
/// and change nothing; the pinned row keeps `IsPinned`.
#[tokio::test]
async fn moves_across_the_pinned_boundary_are_refused() {
    let (dir, db) = temp_db("pinboundary").await;

    common::insert_clip(&db, "alpha", "hash_alpha", "text").await;
    common::insert_clip(&db, "beta", "hash_beta", "text").await;
    common::insert_clip(&db, "gamma", "hash_gamma", "text").await;

    db.with(|conn| cliptoo_core::db::queries::bump_to_bottom(conn, 1))
        .await
        .unwrap();
    assert_eq!(browse_order(&db).await, vec![3, 2, 1]);

    // Pinned clip up past the boundary.
    let blocked = db
        .with(|conn| cliptoo_core::db::queries::move_up_one(conn, 1))
        .await;
    assert!(blocked.is_err(), "must not move the sentinel onto clip 2");
    assert_eq!(browse_order(&db).await, vec![3, 2, 1]);

    // Unpinned clip down past the boundary.
    let blocked = db
        .with(|conn| cliptoo_core::db::queries::move_down_one(conn, 2))
        .await;
    assert!(blocked.is_err(), "must not move clip 2 below the pin");
    assert_eq!(browse_order(&db).await, vec![3, 2, 1]);

    let pinned: i64 = db
        .with(|conn| {
            Ok(
                conn.query_row("SELECT IsPinned FROM clips WHERE Id = 1", [], |row| {
                    row.get(0)
                })?,
            )
        })
        .await
        .unwrap();
    assert_eq!(pinned, 1, "the keep signal travels with the clip");

    clean_up(&dir);
}
