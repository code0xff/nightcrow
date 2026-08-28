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
/// std 가 EINTR 를 내부에서 재시도하는지는 문서화되어 있지 않다.
/// 재시도한다면 이 arm 은 도달하지 않을 뿐 해가 없고, 재시도하지
/// 않는다면 이 arm 이 반드시 필요하다. 문서화되지 않은 동작에
/// 기대는 대신 남겨 둔다.
pub(crate) fn outcome_of(err: &TryLockError) -> Attempt {
    match err {
        TryLockError::WouldBlock => Attempt::Held,
        // 시그널이 호출 중간에 도착해 락을 시도조차 못 했다. 다시 묻는 것만이 옳다.
        TryLockError::Error(err) if err.kind() == std::io::ErrorKind::Interrupted => {
            Attempt::Interrupted
        }
        TryLockError::Error(_) => Attempt::Failed,
    }
}

impl Drop for InstanceLock {
    fn drop(&mut self) {
        // 닫힘만으로도 해제되지만 동기적이지 않다. 명시적 unlock 이 없으면
        // 직전 daemon 이 사라진 직후의 재시작이 "이미 실행 중" 으로 거부되는
        // 경우가 수백 회에 한 번 발생했다.
        let _ = self.file.unlock();
    }
}

#[cfg(test)]
#[path = "lock_tests.rs"]
mod tests;
