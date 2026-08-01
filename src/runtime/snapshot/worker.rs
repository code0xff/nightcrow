//! The reader thread: watch the tree, read it when something changed, and stop
//! when nobody is listening. Bounded by [`MIN_READ_INTERVAL`](super::MIN_READ_INTERVAL)
//! and [`IDLE_READ_INTERVAL`](super::IDLE_READ_INTERVAL).

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
    pub(super) wake_tx: Sender<Wake>,
    pub(super) awake: Arc<AtomicBool>,
    pub(super) watching: Arc<AtomicBool>,
}

/// The watches the worker holds, and what it has already tried to watch.
///
/// What was tried is recorded rather than inferred from the handles: a refusal
/// leaves no handle, and re-deriving "not installed yet" from that would re-walk
/// the tree and log the same warning once a second. A failure is answered by
/// falling back to the interval and retried only when what is wanted changes.
#[derive(Default)]
struct Watches {
    tree: Option<RecommendedWatcher>,
    tree_attempted: bool,
    git_dir: Option<RecommendedWatcher>,
    /// Whether the git directory has been looked for at all, and where it was
    /// when it last was. "nowhere separate to watch" is an answer and must not
    /// read as "not asked yet".
    git_dir_settled: bool,
    git_dir_at: Option<PathBuf>,
}

/// What the worker carries between reads.
struct ReadState {
    /// Cached handle, reopened periodically (see [`REOPEN_REPO_EVERY_READS`]) and
    /// dropped whenever a read fails or the tree stops being watched.
    repo: Option<git2::Repository>,
    reads_since_open: u32,
    changed: bool,
    last_read: Option<Instant>,
}

impl Worker {
    pub(super) fn run(self) {
        // Resolved once: the paths the watcher reports are the filesystem's, and
        // on macOS those differ from the path this was opened with.
        let mut roots = Roots::of(&self.root);
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
                // Not reported as watching yet: whether this tree keeps its git
                // directory elsewhere cannot be asked without a repository
                // handle, and until that second watch is up a linked worktree is
                // told about only half of what can change.
                state.changed = true;
            } else if !awake && watch.tree_attempted {
                watch = Watches::default();
                self.watching.store(false, Ordering::Release);
                // Nor hold a repository handle for a tree nobody is reading.
                state.repo = None;
            }

            if awake && state.changed && due(state.last_read) {
                if !self.read_once(&mut state, &watch) {
                    // The receiver is gone: nobody is left to read this.
                    return;
                }
                // After the read, not before: it takes a repository handle to ask
                // where the git directory is. Every read rather than the first,
                // because the handle is reopened periodically and a checkout that
                // moved comes back with its git directory somewhere else.
                self.follow_git_dir(&mut state, &mut roots, &mut watch);
            }

            match self.wake_rx.recv_timeout(self.wait(awake, &state)) {
                Ok(wake) => {
                    if !self.absorb(wake, &mut state, &roots) {
                        return;
                    }
                    // Take the whole backlog in one pass. A build queues
                    // thousands of events while a single read runs, and both the
                    // memory they sit in and the stop that arrives behind them
                    // wait on this loop reaching them.
                    while let Ok(wake) = self.wake_rx.try_recv() {
                        if !self.absorb(wake, &mut state, &roots) {
                            return;
                        }
                    }
                }
                Err(RecvTimeoutError::Disconnected) => return,
                // The read came due on its own: either the rate limit expired with
                // a change waiting, or the interval that guards against missed
                // events came round.
                Err(RecvTimeoutError::Timeout) => state.changed = true,
            }
        }
    }

    /// Fold one wake into the state. `false` once told to stop.
    fn absorb(&self, wake: Wake, state: &mut ReadState, roots: &Roots) -> bool {
        match wake {
            // Already owed a read: asking git about every further path would be
            // work whose answer cannot change what happens next.
            Wake::Changed(paths) => {
                if !state.changed && snapshot_watch::any_matters(state.repo.as_ref(), roots, &paths)
                {
                    state.changed = true;
                }
                true
            }
            Wake::Stop => false,
        }
    }

    /// Keep a second watch on the git directory when it lives outside the work
    /// tree — `git worktree add` and `--separate-git-dir` both do. The index
    /// lives there, so without this a `git add` goes unseen until the idle read.
    ///
    /// Also settles `watching`, which says whether *every* place a change can
    /// come from is watched: it is what
    /// [`SnapshotChannel::is_watching`](super::SnapshotChannel::is_watching)
    /// reports and what [`wait`](Self::wait) reads to choose an interval.
    fn follow_git_dir(&self, state: &mut ReadState, roots: &mut Roots, watch: &mut Watches) {
        let Some(repo) = state.repo.as_ref() else {
            // No handle to ask, and nothing to correct: the read that dropped it
            // has already said what is being watched.
            return;
        };
        // None means it is inside the work tree, where the recursive watch
        // already covers it and a second one would double every event.
        let wanted = snapshot_watch::external_git_dir(repo, roots);
        if watch.git_dir_settled && wanted == watch.git_dir_at {
            return;
        }
        watch.git_dir_settled = true;
        watch.git_dir_at = wanted.clone();
        roots.set_external_git_dir(wanted.as_deref());
        watch.git_dir = wanted
            .as_ref()
            .and_then(|dir| snapshot_watch::install(dir, self.wake_tx.clone()));
        if watch.git_dir.is_some() {
            // The read just taken happened before this watch existed, and a
            // `git add` in that gap left no trace anywhere else.
            state.changed = true;
        }
        let complete = watch.tree.is_some() && (wanted.is_none() || watch.git_dir.is_some());
        self.watching.store(complete, Ordering::Release);
    }

    /// Read the tree and send what it says. `false` once the receiver is gone.
    ///
    /// `watch` is read for one answer only: what is left watching a directory
    /// that turns out to hold no repository — see the failure below.
    fn read_once(&self, state: &mut ReadState, watch: &Watches) -> bool {
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
                    // A directory that is not a repository yet may become one,
                    // but leaving `changed` standing to retry held the reader at
                    // `MIN_READ_INTERVAL` and sent this same error every second.
                    // The watch notices `git init` and checkouts appearing, so
                    // the idle interval is enough — see [`wait`](Self::wait).
                    state.changed = false;
                    self.watching.store(watch.tree.is_some(), Ordering::Release);
                    state.last_read = Some(Instant::now());
                    let msg = SnapshotMsg::Err(format!("not a git repository: {err}"));
                    return self.deliver(msg);
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
        self.deliver(msg)
    }

    /// Hand a reading over, unless nobody is reading any more. `false` once the
    /// receiver is gone.
    ///
    /// Checked again here rather than only before the walk, which on a large
    /// tree takes long enough for the last client to leave. A reading nobody
    /// waited for is worse than wasted: it sits in the channel until whoever
    /// owns the receiver next drains it, and that is after the next client has
    /// taken a fresher reading for itself and shown it. The older one then lands
    /// on top.
    fn deliver(&self, msg: SnapshotMsg) -> bool {
        if !self.awake.load(Ordering::Acquire) {
            return true;
        }
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

#[cfg(test)]
#[path = "worker_tests.rs"]
mod tests;
