//! Bounded thread-reaping helpers, and the wake signal a worker parks on
//! instead of polling.

use std::sync::{Arc, Condvar, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

/// A shared handle to a [`Wake`]. Cheap to clone, and any producer thread may
/// hold one.
pub type WakeHandle = Arc<Wake>;

/// A coalescing wake signal between any number of producers and one consumer.
///
/// Producers call [`wake`](Self::wake) after queueing work; the consumer calls
/// [`wait`](Self::wait) instead of sleeping. A signal that arrives while the
/// consumer is running sets a flag its next wait returns on, so a producer never
/// has to know whether the consumer is parked — which is what makes "check for
/// work, then wait" free of lost wakeups. Repeated signals before the next wait
/// collapse into one, so a burst costs one extra pass rather than one per call.
///
/// `wait` never blocks past its cap, so a producer that forgets to signal, or a
/// deadline the consumer computed before a state change it cannot see, costs
/// latency rather than correctness.
#[derive(Default)]
pub struct Wake {
    pending: Mutex<bool>,
    ready: Condvar,
}

impl Wake {
    /// Report that work is queued. Never blocks on a consumer that is already
    /// running; a consumer between passes keeps the flag for its next wait.
    pub fn wake(&self) {
        let mut pending = self
            .pending
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        *pending = true;
        self.ready.notify_all();
    }

    /// Wait until signalled, until `deadline`, or for at most `cap` — whichever
    /// comes first — and return at once when a signal is already pending.
    ///
    /// `deadline` in the past returns immediately: the caller's next pass is what
    /// observes it, so the state behind a deadline has to be cleared by that pass
    /// or the wait spins.
    pub fn wait(&self, deadline: Option<Instant>, cap: Duration) {
        let started = Instant::now();
        let until = deadline.unwrap_or(started + cap).min(started + cap);
        let timeout = until.saturating_duration_since(started);
        let mut pending = self
            .pending
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if *pending || timeout.is_zero() {
            *pending = false;
            return;
        }
        let (mut pending, _) = self
            .ready
            .wait_timeout(pending, timeout)
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        *pending = false;
    }
}

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
