//! Bounded thread-reaping helpers.

use std::sync::{Arc, Mutex};
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

/// Run `job` on its own detached thread.
///
/// For a caller that has work which waits — a `Drop` that still has to kill a
/// process tree and give worker threads their reap window — but must not spend
/// that wait itself. Nothing joins the thread: the job owns everything it needs
/// until it is done.
///
/// A failed spawn runs the job on the caller instead. The alternative would drop
/// it, which for teardown means leaking a process tree precisely when the system
/// is already short of resources.
pub fn spawn_reaper(name: &str, job: impl FnOnce() + Send + 'static) {
    let pending = Arc::new(Mutex::new(Some(job)));
    let worker = Arc::clone(&pending);
    let spawned = thread::Builder::new()
        .name(name.to_string())
        .spawn(move || {
            let job = worker.lock().unwrap_or_else(|e| e.into_inner()).take();
            if let Some(job) = job {
                job();
            }
        });
    if let Err(error) = spawned {
        tracing::warn!(%error, "could not start {name}; doing its work on this thread");
        let job = pending.lock().unwrap_or_else(|e| e.into_inner()).take();
        if let Some(job) = job {
            job();
        }
    }
}

#[cfg(test)]
#[path = "threading_tests.rs"]
mod tests;
