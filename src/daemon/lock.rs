//! The single-instance lock. Two daemons on one socket would each serve half
//! the attaching clients, and the second to bind would displace the first.
//! An advisory lock decides — not the socket, where a `connect` that succeeds
//! does not prove a listener is alive (macOS can succeed against a socket whose
//! listener has closed). The kernel holds the lock until the process ends —
//! including a `kill -9` — so holding it means "no other daemon is live" with
//! no race and no timeout.

use anyhow::{Context, Result};
use std::fs::{File, OpenOptions, TryLockError};
use std::path::Path;

/// An exclusive claim on being *the* daemon, released when dropped or when the
/// process ends.
#[derive(Debug)]
pub struct InstanceLock {
    /// Held open for its lock, and released explicitly on the way out — see the
    /// `Drop` impl for why closing it is not enough.
    file: File,
}

impl InstanceLock {
    /// Take the lock, or report that another daemon holds it. The lock file is
    /// never removed — unlinking it on release would let a second daemon lock a
    /// file the first had already deleted, and both would then believe they hold
    /// it. An empty file left behind costs nothing; the lock is the advisory
    /// lock, not the file.
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
            match file.try_lock() {
                Ok(()) => return Ok(Some(Self { file })),
                Err(err) => match outcome_of(&err) {
                    Attempt::Held => return Ok(None),
                    Attempt::Interrupted => continue,
                    Attempt::Failed => {
                        return Err(anyhow::Error::new(err))
                            .with_context(|| format!("locking {}", path.display()));
                    }
                },
            }
        }
    }
}

/// What a failed lock means for the caller.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Attempt {
    /// Another daemon holds it.
    Held,
    /// A signal arrived mid-call and the lock was never attempted. Says nothing
    /// about who holds what, so the only correct response is to ask again.
    Interrupted,
    /// Something else went wrong.
    Failed,
}

/// Read a lock failure. `Interrupted` earns its own arm because this process
/// raises signals at itself — a stop signal is how the daemon is asked to shut
/// down — and EINTR says nothing about who holds the lock.
///
/// The standard library's handling of EINTR is undocumented. If it retries
/// internally, this arm is merely unreachable; if it does not, the arm is
/// required. Keep it rather than relying on undocumented behaviour.
pub(crate) fn outcome_of(err: &TryLockError) -> Attempt {
    match err {
        TryLockError::WouldBlock => Attempt::Held,
        // A signal arrived before the lock was attempted; retrying is the only
        // correct response.
        TryLockError::Error(err) if err.kind() == std::io::ErrorKind::Interrupted => {
            Attempt::Interrupted
        }
        TryLockError::Error(_) => Attempt::Failed,
    }
}

impl Drop for InstanceLock {
    fn drop(&mut self) {
        // Closing releases the lock eventually, but an explicit unlock avoids
        // an immediate restart being rejected as "already running" while the
        // previous daemon's close is still propagating.
        let _ = self.file.unlock();
    }
}

#[cfg(test)]
#[path = "lock_tests.rs"]
mod tests;
