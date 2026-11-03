//! Limit for full-size image decodes.
//!
//! A clipboard image, a file thumbnail and a hover preview each decode a whole
//! frame into memory. Left unbounded, several copies in flight can spike RSS by
//! hundreds of MB, and the freed buffers still fragment the heap afterwards.
//! One decode at a time is enough for interactive use and keeps the peak
//! bounded to one frame.

use std::sync::OnceLock;

use tokio::sync::{Semaphore, SemaphorePermit};

/// Full-size decodes allowed to run at once.
const MAX_CONCURRENT_DECODES: usize = 1;

fn limit() -> &'static Semaphore {
    static LIMIT: OnceLock<Semaphore> = OnceLock::new();
    LIMIT.get_or_init(|| Semaphore::new(MAX_CONCURRENT_DECODES))
}

/// Hold the returned permit for the whole decode.
pub async fn permit() -> SemaphorePermit<'static> {
    limit()
        .acquire()
        .await
        .expect("decode semaphore is never closed")
}
