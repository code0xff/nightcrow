//! The single-instance lock.
//!
//! Two daemons on one socket would each serve half the attaching clients, and
//! the second to bind would displace the first. Deciding which is running has
//! to be exact, which rules out asking the socket: a `connect` that succeeds
//! does not prove a listener is alive — on macOS it can succeed against a
//! socket whose listener has closed, and the reset only shows up on the next
//! read. Probing that way was flaky in exactly the case it exists for.
//!
//! An advisory `flock` answers instead. The kernel holds it for as long as the
//! descriptor is open and releases it when the process ends — including a
//! `kill -9`, where no cleanup code of ours runs. So holding the lock means
//! "no other daemon is live" with no race and no timeout.

use anyhow::{Context, Result};
use std::fs::{File, OpenOptions};
use std::os::fd::AsRawFd;
use std::path::Path;

/// An exclusive claim on being *the* daemon, released when dropped or when the
/// process ends.
#[derive(Debug)]
pub struct InstanceLock {
    /// Held open for its `flock`; closing the descriptor releases the lock.
    /// Nothing reads it — the value exists to keep the descriptor alive.
    _file: File,
}

impl InstanceLock {
    /// Take the lock, or report that another daemon holds it.
    ///
    /// The lock file is never removed. Unlinking it on release would let a
    /// second daemon lock a file the first had already deleted, and both would
    /// then believe they hold it — the classic lockfile race. An empty file
    /// left behind costs nothing; the lock is the `flock`, not the file.
    pub fn acquire(path: &Path) -> Result<Option<Self>> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("creating the daemon directory {}", parent.display()))?;
        }
        let file = OpenOptions::new()
            .create(true)
            .read(true)
            .write(true)
            .truncate(false)
            .open(path)
            .with_context(|| format!("opening the daemon lock {}", path.display()))?;
        // SAFETY: `flock` takes a valid descriptor, which `file` owns for the
        // whole call and beyond — the lock lives with the descriptor, so the
        // file is kept in the returned value rather than dropped here.
        let locked = unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) };
        if locked != 0 {
            let err = std::io::Error::last_os_error();
            // EWOULDBLOCK is the answer this exists to get: someone else holds
            // it. Anything else is a real failure and must not be read as
            // "another daemon is running".
            if err.kind() == std::io::ErrorKind::WouldBlock {
                return Ok(None);
            }
            return Err(err).with_context(|| format!("locking {}", path.display()));
        }
        Ok(Some(Self { _file: file }))
    }
}

#[cfg(test)]
#[path = "lock_tests.rs"]
mod tests;
