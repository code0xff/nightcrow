//! Shared low-level utilities. Keep tiny — anything domain-specific should
//! live in the relevant module instead.

use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

/// Default reap window for [`try_timed_join`] at known-quiescent call sites
/// (Drop impls, worker swap-out). The signal-then-join pattern means the
/// worker is already a few syscalls from returning; a handful of millis is
/// generous without ever stalling the UI noticeably.
pub const REAP_TIMEOUT: Duration = Duration::from_millis(5);

/// Spin briefly waiting for `handle` to finish, then either join it or
/// detach the handle. Detaches without panicking on timeout so the UI is
/// never blocked by a hung worker. Used at quiescent moments (repo switch,
/// after reply drain, Drop) where the worker is either already done or
/// microseconds away from exiting.
pub fn try_timed_join(handle: JoinHandle<()>, timeout: Duration) {
    let deadline = Instant::now() + timeout;
    while !handle.is_finished() && Instant::now() < deadline {
        // Short sleep keeps the busy-wait cheap; the common path is one
        // iteration because the worker exits as soon as it tries to send.
        thread::sleep(Duration::from_micros(200));
    }
    if handle.is_finished() {
        if let Err(e) = handle.join() {
            tracing::warn!(?e, "worker thread panicked");
        }
    } else {
        tracing::debug!("worker still running at detach; reaping deferred to OS");
        drop(handle);
    }
}
