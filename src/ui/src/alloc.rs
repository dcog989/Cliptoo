//! Glibc allocator tuning.
//!
//! Copying images left the process ~300 MB resident with nothing live: glibc
//! raises its mmap threshold after every large free (up to 32 MiB), so later
//! image-sized buffers are carved from the heap and never returned. Tokio's
//! blocking threads are pooled, so their arenas persist and each copied 4K
//! image left ~33 MiB (one RGBA frame) of dead heap behind. Pinning the
//! thresholds makes every freed buffer of 128 KiB or more go back to the OS.
//!
//! Crate roots `#![deny(unsafe_code)]`; this is the second carve-out beside
//! `drag.rs`, for the same reason: calling the C library's own API.
#![allow(unsafe_code)]

/// Free-heap size from which glibc returns memory to the OS instead of keeping
/// it, for both the mmap and the brk heap. glibc's own guidance value; the
/// dynamic default (up to 32 MiB) is what retained the image frames.
const THRESHOLD_BYTES: libc::c_int = 128 * 1024;

/// Pin glibc's mmap and trim thresholds. Call once, before large buffers are
/// allocated; setting either disables glibc's dynamic threshold adjustment,
/// which is the point. A no-op where `mallopt` is unimplemented (musl).
pub fn tune() {
    unsafe {
        libc::mallopt(libc::M_MMAP_THRESHOLD, THRESHOLD_BYTES);
        libc::mallopt(libc::M_TRIM_THRESHOLD, THRESHOLD_BYTES);
    }
}
