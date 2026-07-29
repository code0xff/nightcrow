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
    /// Held open for its `flock`, and released through explicitly on the way
    /// out — see the `Drop` impl for why closing it is not enough.
    file: File,
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
        loop {
            // SAFETY: `flock` takes a valid descriptor, which `file` owns for
            // the whole call and beyond — the lock lives with the descriptor,
            // so the file is kept in the returned value rather than dropped
            // here.
            let locked = unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) };
            if locked == 0 {
                return Ok(Some(Self { file }));
            }
            let err = std::io::Error::last_os_error();
            match outcome_of(&err) {
                Attempt::Held => return Ok(None),
                Attempt::Interrupted => continue,
                Attempt::Failed => {
                    return Err(err).with_context(|| format!("locking {}", path.display()));
                }
            }
        }
    }
}

/// What a failed `flock` means for the caller.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Attempt {
    /// Another daemon holds it. The normal negative answer.
    Held,
    /// A signal arrived mid-call and the lock was never attempted. Says
    /// nothing about who holds what, so the only correct response is to ask
    /// again.
    Interrupted,
    /// Something else went wrong, and it must not be reported as either of the
    /// above.
    Failed,
}

/// Read a `flock` failure.
///
/// `Interrupted` earns its own arm because this process raises signals at
/// itself — a stop signal is how the daemon is asked to shut down. A signal
/// landing on the thread inside `flock` returns EINTR, which says nothing
/// about who holds the lock; reported as a failure it would refuse to start a
/// daemon for no reason.
fn outcome_of(err: &std::io::Error) -> Attempt {
    match err.kind() {
        std::io::ErrorKind::WouldBlock => Attempt::Held,
        std::io::ErrorKind::Interrupted => Attempt::Interrupted,
        _ => Attempt::Failed,
    }
}

impl Drop for InstanceLock {
    /// Release the lock before the descriptor closes.
    ///
    /// Closing does release it — but not synchronously. A `flock` on a freshly
    /// opened descriptor a millisecond later can still see the lock held,
    /// which showed up as a daemon refusing to start with "already running"
    /// moments after the previous one had gone, roughly once in every few
    /// hundred stop-and-start cycles. `LOCK_UN` releases before this returns,
    /// so the next daemon's attempt cannot race the last one's exit.
    fn drop(&mut self) {
        loop {
            // SAFETY: `flock` takes a valid descriptor, which `self.file` owns
            // until this returns and the field is dropped after it.
            if unsafe { libc::flock(self.file.as_raw_fd(), libc::LOCK_UN) } == 0 {
                return;
            }
            // Retried for the same reason acquiring is: a signal mid-call
            // leaves the lock exactly as it was. Anything else is not worth
            // failing a shutdown over — the descriptor is about to close,
            // which releases it the slower way.
            if std::io::Error::last_os_error().kind() != std::io::ErrorKind::Interrupted {
                return;
            }
        }
    }
}

#[cfg(test)]
#[path = "lock_tests.rs"]
mod tests;
