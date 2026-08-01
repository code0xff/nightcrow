//! The single-instance lock.
//!
//! Two daemons on one socket would each serve half the attaching clients, and
//! the second to bind would displace the first. Deciding which is running has
//! to be exact, which rules out asking the socket: a `connect` that succeeds
//! does not prove a listener is alive — on macOS it can succeed against a
//! socket whose listener has closed, and the reset only shows up on the next
//! read. Probing that way was flaky in exactly the case it exists for.
//!
//! An advisory lock answers instead. The kernel holds it for as long as the
//! descriptor is open and releases it when the process ends — including a
//! `kill -9`, where no cleanup code of ours runs. So holding the lock means
//! "no other daemon is live" with no race and no timeout.

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
    /// Take the lock, or report that another daemon holds it.
    ///
    /// The lock file is never removed. Unlinking it on release would let a
    /// second daemon lock a file the first had already deleted, and both would
    /// then believe they hold it — the classic lockfile race. An empty file
    /// left behind costs nothing; the lock is the advisory lock, not the file.
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

/// Read a lock failure.
///
/// `Interrupted` earns its own arm because this process raises signals at
/// itself — a stop signal is how the daemon is asked to shut down. A signal
/// landing on the thread inside the lock call returns EINTR, which says nothing
/// about who holds the lock; reported as a failure it would refuse to start a
/// daemon for no reason.
///
/// std 가 EINTR 를 내부에서 재시도하는지는 문서화되어 있지 않다.
/// 재시도한다면 이 arm 은 도달하지 않을 뿐 해가 없고, 재시도하지
/// 않는다면 이 arm 이 반드시 필요하다. 문서화되지 않은 동작에
/// 기대는 대신 남겨 둔다.
pub(crate) fn outcome_of(err: &TryLockError) -> Attempt {
    match err {
        // 다른 daemon 이 쥐고 있다. 정상적인 부정 응답.
        TryLockError::WouldBlock => Attempt::Held,
        // 시그널이 호출 중간에 도착해 락을 시도조차 못 했다. 누가 무엇을
        // 쥐고 있는지 아무 말도 하지 않으므로 다시 묻는 것만이 옳다.
        TryLockError::Error(err) if err.kind() == std::io::ErrorKind::Interrupted => {
            Attempt::Interrupted
        }
        TryLockError::Error(_) => Attempt::Failed,
    }
}

impl Drop for InstanceLock {
    /// Release the lock before the descriptor closes.
    ///
    /// Closing does release it — but not synchronously. A lock on a freshly
    /// opened descriptor a millisecond later can still see the lock held,
    /// which showed up as a daemon refusing to start with "already running"
    /// moments after the previous one had gone, roughly once in every few
    /// hundred stop-and-start cycles. `unlock` releases before this returns,
    /// so the next daemon's attempt cannot race the last one's exit.
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
