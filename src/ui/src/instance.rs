//! Single-instance lock.
//!
//! The KWin tracker script has a fixed name and the portal shortcut id is
//! fixed, so a second process would unload the first instance's script and
//! register a duplicate shortcut. A pid file created with `create_new` makes
//! the first process the only one; a stale file (crash/kill) is detected via
//! `/proc` and replaced.

use anyhow::{Context, Result};
use std::io::Write;
use std::path::Path;

/// Matches the binary name in Cargo.toml.
const PROCESS_NAME: &str = "cliptoo";

/// How long to wait for the lock owner to write its pid before calling a
/// just-created lock stale. Without it, a second launch can read the file
/// between `create_new` and the owner's first write, delete a live lock, and
/// run alongside the first process.
const PID_WRITE_GRACE: std::time::Duration = std::time::Duration::from_millis(100);
const PID_POLL: std::time::Duration = std::time::Duration::from_millis(20);

/// Try to become the one running instance. Returns `false` when another live
/// Cliptoo process holds the lock.
pub fn acquire(path: &Path) -> Result<bool> {
    for _ in 0..2 {
        match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)
        {
            Ok(mut file) => {
                writeln!(file, "{}", std::process::id())
                    .with_context(|| format!("writing lock file {}", path.display()))?;
                return Ok(true);
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                if !lock_is_stale(path) {
                    return Ok(false);
                }
                match std::fs::remove_file(path) {
                    Ok(()) => {}
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                    Err(e) => {
                        return Err(e)
                            .with_context(|| format!("removing stale lock {}", path.display()));
                    }
                }
            }
            Err(e) => return Err(e).with_context(|| format!("creating lock {}", path.display())),
        }
    }
    Ok(false)
}

fn running_pid(path: &Path) -> Option<u32> {
    std::fs::read_to_string(path).ok()?.trim().parse().ok()
}

/// Whether the lock belongs to a process that is not running Cliptoo. An empty
/// or unreadable pid gets a short grace period: the owner may still be writing
/// it after `create_new`.
fn lock_is_stale(path: &Path) -> bool {
    let deadline = std::time::Instant::now() + PID_WRITE_GRACE;
    loop {
        if let Some(pid) = running_pid(path) {
            return !is_cliptoo(pid);
        }
        if std::time::Instant::now() >= deadline {
            return true;
        }
        std::thread::sleep(PID_POLL);
    }
}

fn is_cliptoo(pid: u32) -> bool {
    // A pid recycled by an unrelated process makes the lock stale. Our own pid
    // is live by definition, which also makes the unit test work without a
    // second binary.
    pid == std::process::id()
        || std::fs::read_to_string(format!("/proc/{pid}/comm"))
            .is_ok_and(|name| name.trim().starts_with(PROCESS_NAME))
}

#[cfg(test)]
mod tests {
    use super::acquire;

    #[test]
    fn a_second_acquire_is_rejected_until_the_lock_is_removed() {
        let dir = std::env::temp_dir().join(format!("cliptoo_instance_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let lock = dir.join("cliptoo.lock");
        let _ = std::fs::remove_file(&lock);

        assert!(acquire(&lock).unwrap());
        assert!(!acquire(&lock).unwrap());

        // A stale pid is replaced by the next process.
        std::fs::write(&lock, "999999999\n").unwrap();
        assert!(acquire(&lock).unwrap());

        // A lock with no pid yet (the owner is between create and write) is
        // given a grace period, then replaced instead of blocking forever.
        std::fs::write(&lock, "").unwrap();
        assert!(acquire(&lock).unwrap());

        std::fs::remove_dir_all(&dir).unwrap();
    }
}
