//! The reader thread: watch the tree, read it when something changed, and stop
//! when nobody is listening.
//!
//! Everything here answers one question — is it time to read? — from three
//! inputs: what the filesystem reported, how long ago the last read was, and
//! whether anyone is reading at all. The bounds it works within are documented on
//! [`MIN_READ_INTERVAL`](super::MIN_READ_INTERVAL) and
//! [`IDLE_READ_INTERVAL`](super::IDLE_READ_INTERVAL).

use super::{IDLE_READ_INTERVAL, MIN_READ_INTERVAL, REOPEN_REPO_EVERY_READS, SnapshotMsg, read};
use crate::runtime::snapshot_watch::{self, Roots, Wake};
use notify::RecommendedWatcher;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender};
use std::time::{Duration, Instant};

/// The background reader: watches the tree, and reads it when something changed.
pub(super) struct Worker {
    pub(super) root: PathBuf,
    pub(super) tx: Sender<SnapshotMsg>,
    pub(super) wake_rx: Receiver<Wake>,
    /// Handed to each watcher it installs.
    pub(super) wake_tx: Sender<Wake>,
    pub(super) awake: Arc<AtomicBool>,
    pub(super) watching: Arc<AtomicBool>,
}

/// The watch the worker holds, and whether it has already tried to install it.
///
/// The attempt is recorded rather than inferred from the handle: a refusal
/// leaves no handle, and re-deriving "not installed yet" from that would re-walk
/// the tree and log the same warning once a second for as long as the session
/// lives. A failure is answered by falling back to the interval — see
/// [`install`](snapshot_watch::install) — and retried only when a repository
/// nobody was reading is picked up again.
#[derive(Default)]
struct Watches {
    tree: Option<RecommendedWatcher>,
    tree_attempted: bool,
}

/// What the worker carries between reads.
struct ReadState {
    /// Cached handle, reopened periodically (see [`REOPEN_REPO_EVERY_READS`]) and
    /// dropped whenever a read fails or the tree stops being watched.
    repo: Option<git2::Repository>,
    reads_since_open: u32,
    /// Something happened that has not been read yet.
    changed: bool,
    last_read: Option<Instant>,
}

impl Worker {
    pub(super) fn run(self) {
        // Resolved once: the paths the watcher reports are the filesystem's, and
        // on macOS those differ from the path this was opened with.
        let roots = Roots::of(&self.root);
        let mut watch = Watches::default();
        let mut state = ReadState {
            repo: None,
            reads_since_open: 0,
            // The first read is owed: a client that just opened this repository
            // has nothing to render until it arrives.
            changed: true,
            last_read: None,
        };

        loop {
            let awake = self.awake.load(Ordering::Acquire);
            if awake && !watch.tree_attempted {
                watch.tree_attempted = true;
                watch.tree = snapshot_watch::install(&self.root, self.wake_tx.clone());
                self.watching.store(watch.tree.is_some(), Ordering::Release);
                // Whatever happened while the watch was off went unseen.
                state.changed = true;
            } else if !awake && watch.tree_attempted {
                watch = Watches::default();
                self.watching.store(false, Ordering::Release);
                // Nor hold a repository handle for a tree nobody is reading.
                state.repo = None;
            }

            if awake && state.changed && due(state.last_read) && !self.read_once(&mut state) {
                // The receiver is gone: nobody is left to read this.
                return;
            }

            match self.wake_rx.recv_timeout(self.wait(awake, &state)) {
                Ok(Wake::Changed(paths)) => {
                    if snapshot_watch::any_matters(state.repo.as_ref(), &roots, &paths) {
                        state.changed = true;
                    }
                }
                Ok(Wake::Stop) | Err(RecvTimeoutError::Disconnected) => return,
                // The read came due on its own: either the rate limit expired with
                // a change waiting, or the interval that guards against missed
                // events came round.
                Err(RecvTimeoutError::Timeout) => state.changed = true,
            }
        }
    }

    /// Read the tree and send what it says. `false` once the receiver is gone.
    fn read_once(&self, state: &mut ReadState) -> bool {
        if state.reads_since_open >= REOPEN_REPO_EVERY_READS {
            state.repo = None;
        }
        if state.repo.is_none() {
            match git2::Repository::discover(&self.root) {
                Ok(opened) => {
                    state.repo = Some(opened);
                    state.reads_since_open = 0;
                }
                Err(err) => {
                    // `changed` is left standing, so the next permitted slot
                    // retries — a directory that is not a repository yet may
                    // become one.
                    state.last_read = Some(Instant::now());
                    let msg = SnapshotMsg::Err(format!("not a git repository: {err}"));
                    return self.tx.send(msg).is_ok();
                }
            }
        }
        let Some(open) = state.repo.as_ref() else {
            return true;
        };
        let msg = match read(open) {
            Ok((snapshot, mtimes)) => SnapshotMsg::Ok(snapshot, mtimes),
            Err(err) => {
                // Drop the handle: the next read re-discovers. This covers a
                // repository that was relocated or whose internal state became
                // inconsistent. The reopen counter goes with it so the next
                // successful open restarts the cycle cleanly.
                state.repo = None;
                state.reads_since_open = 0;
                SnapshotMsg::Err(err.to_string())
            }
        };
        state.reads_since_open += 1;
        state.changed = false;
        state.last_read = Some(Instant::now());
        self.tx.send(msg).is_ok()
    }

    /// How long to wait before looking again, absent an event.
    fn wait(&self, awake: bool, state: &ReadState) -> Duration {
        if !awake {
            // Only to re-check the flag; `set_awake` wakes this thread anyway.
            return IDLE_READ_INTERVAL;
        }
        let Some(last) = state.last_read else {
            return Duration::ZERO;
        };
        let target = if state.changed {
            MIN_READ_INTERVAL
        } else if self.watching.load(Ordering::Acquire) {
            IDLE_READ_INTERVAL
        } else {
            // Nothing will report a change, so this is the old fixed interval.
            MIN_READ_INTERVAL
        };
        (last + target).saturating_duration_since(Instant::now())
    }
}

/// Whether enough time has passed since the last read for another.
fn due(last_read: Option<Instant>) -> bool {
    last_read.is_none_or(|last| last.elapsed() >= MIN_READ_INTERVAL)
}
