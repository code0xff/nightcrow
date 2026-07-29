use crate::git::diff::{RepoSnapshot, load_snapshot};
use crate::runtime::snapshot_watch::Wake;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;
use std::time::{Duration, SystemTime};

mod worker;

use worker::Worker;

/// Owns the receiver and wake channel for the background snapshot thread.
/// Dropping the struct signals the worker to exit and joins it before
/// returning, so a repo switch cannot leave the old-repo worker holding a
/// `git2::Repository` after the new channel is in place.
pub struct SnapshotChannel {
    rx: Receiver<SnapshotMsg>,
    /// Cleared to stop reading the tree without stopping the worker.
    ///
    /// A `git status` is not free and one runs per channel. A caller that knows
    /// nobody is reading — the viewer with no client subscribed — turns it off
    /// rather than paying for snapshots that go straight in the bin. The
    /// filesystem watch goes with it, so a repository nobody reads holds no
    /// watch descriptors either.
    awake: Arc<AtomicBool>,
    /// Whether the worker is being told about *every* place a change can come
    /// from, rather than looking on a timer. False while asleep, on a tree the
    /// watcher could not install on, and — until the first read answers where the
    /// git directory is — on a checkout that keeps it outside the work tree.
    ///
    /// Nothing in production reads this — a failed watch is reported where it
    /// happens, and the reader behaves correctly either way. It exists so the
    /// tests about *absence* (an idle repository is not walked) can tell "the
    /// watch is doing its job" apart from "this machine could not watch at all".
    #[cfg(test)]
    watching: Arc<AtomicBool>,
    /// Wakes the worker: filesystem events, resumption, and the stop on drop.
    /// One channel for all three, so an idle repository costs no wake-ups beyond
    /// the interval that guards against missed events.
    ///
    /// Held in an `Option` so `Drop` can release it before joining the worker.
    wake: Option<Sender<Wake>>,
    // None in test fixtures that construct an inert channel via
    // `from_endpoints` (no real worker to join).
    handle: Option<thread::JoinHandle<()>>,
}

/// Shortest gap between two reads.
///
/// The rate limit is what makes watching safe: a tree that churns without pause
/// costs exactly what the old fixed-interval poll cost and never more. Equal to
/// that old interval for that reason.
const MIN_READ_INTERVAL: Duration = Duration::from_millis(1000);

/// Longest gap between two reads while awake and watching.
///
/// A watcher can miss an event, or install on part of a tree and fail on the
/// rest, and "stale until the user happens to change something else" is not a
/// state to leave a file list in. With no watcher at all this is not used: the
/// reader falls back to [`MIN_READ_INTERVAL`], which is what it did before.
const IDLE_READ_INTERVAL: Duration = Duration::from_secs(10);

/// Reopen the cached `git2::Repository` handle every N reads so we observe
/// out-of-band repo changes (e.g. `git gc`, packfile rewrites, worktree moves)
/// that the cached handle would otherwise serve stale. Counted in reads rather
/// than in seconds now that reads follow changes — a repository nobody touches
/// is not read, and does not need reopening either.
const REOPEN_REPO_EVERY_READS: u32 = 30;

impl SnapshotChannel {
    /// Start reading `repo_path` at once, for an owner that is about to show it.
    pub fn spawn(repo_path: &str) -> Self {
        Self::start(repo_path, true)
    }

    /// Start without reading, for an owner that knows nobody is looking yet.
    ///
    /// Separate from `spawn` followed by `set_awake(false)`, which is a race the
    /// worker can win: it reads before that clears, which walks a tree nobody
    /// asked about and leaves the reading queued to be published after a later,
    /// newer one. The daemon opens every repository in a session and the browser
    /// subscribes to one of them, so this is the ordinary case, not the odd one.
    pub fn spawn_asleep(repo_path: &str) -> Self {
        Self::start(repo_path, false)
    }

    fn start(repo_path: &str, awake: bool) -> Self {
        let (tx, rx) = mpsc::channel::<SnapshotMsg>();
        let (wake_tx, wake_rx) = mpsc::channel::<Wake>();
        let awake = Arc::new(AtomicBool::new(awake));
        let watching = Arc::new(AtomicBool::new(false));
        #[cfg(test)]
        let channel_watching = Arc::clone(&watching);
        let worker = Worker {
            root: PathBuf::from(repo_path),
            tx,
            wake_rx,
            wake_tx: wake_tx.clone(),
            awake: Arc::clone(&awake),
            watching: Arc::clone(&watching),
        };
        let handle = thread::spawn(move || worker.run());
        Self {
            rx,
            awake,
            #[cfg(test)]
            watching: channel_watching,
            wake: Some(wake_tx),
            handle: Some(handle),
        }
    }

    /// A handle for turning the reading on and off from another thread.
    ///
    /// Separate from the channel because the channel owns a receiver and cannot
    /// be shared, while whoever decides that nobody is reading — a server
    /// counting its subscribers — is on a different thread from the one draining
    /// it.
    pub fn watch(&self) -> SnapshotWatch {
        SnapshotWatch {
            awake: Arc::clone(&self.awake),
            wake: self.wake.clone(),
        }
    }

    /// Whether the worker is being told about every place a change can come
    /// from, rather than reading on a timer. See the field.
    #[cfg(test)]
    pub(crate) fn is_watching(&self) -> bool {
        self.watching.load(Ordering::Acquire)
    }

    /// One snapshot read on the calling thread, for a caller that needs an answer
    /// now rather than when the next change arrives.
    pub fn read_now(repo_path: &str) -> Option<(RepoSnapshot, HashMap<String, SystemTime>)> {
        let repo = git2::Repository::discover(repo_path).ok()?;
        read(&repo).ok()
    }

    pub fn try_recv(&self) -> Result<SnapshotMsg, mpsc::TryRecvError> {
        self.rx.try_recv()
    }

    /// Build a `SnapshotChannel` from an externally provided receiver. Lets
    /// tests construct an inert channel (no worker thread, no watcher) so they
    /// can inject snapshots directly instead of booting the background reader.
    #[cfg(test)]
    pub(crate) fn from_endpoints(rx: Receiver<SnapshotMsg>) -> Self {
        Self {
            rx,
            awake: Arc::new(AtomicBool::new(true)),
            watching: Arc::new(AtomicBool::new(false)),
            wake: None,
            handle: None,
        }
    }
}

/// Turns a [`SnapshotChannel`]'s reading on and off. Takes effect at once.
#[derive(Clone)]
pub struct SnapshotWatch {
    awake: Arc<AtomicBool>,
    wake: Option<Sender<Wake>>,
}

impl SnapshotWatch {
    /// Whether the reading is on. For a test whose claim is that some sequence
    /// of callers left it in the right state — the count of who wants it read is
    /// not the same fact as whether it is being read.
    #[cfg(test)]
    pub(crate) fn is_awake(&self) -> bool {
        self.awake.load(Ordering::Acquire)
    }

    pub fn set_awake(&self, awake: bool) {
        self.awake.store(awake, Ordering::Release);
        // Woken rather than left to the interval: resuming means a client is
        // waiting to see this repository, and it must not sit behind a timer that
        // exists for missed events.
        if let Some(wake) = &self.wake {
            let _ = wake.send(Wake::Changed(Vec::new()));
        }
    }
}

impl Drop for SnapshotChannel {
    fn drop(&mut self) {
        // Release the wake sender first: the worker's `recv_timeout` observes the
        // stop immediately rather than sitting out the idle interval.
        if let Some(wake) = self.wake.take() {
            let _ = wake.send(Wake::Stop);
        }
        // Wait for the worker to finish its current `load_snapshot` so a
        // `change_repo` doesn't leave the old-repo worker running with a
        // live `git2::Repository` after the new channel is installed.
        // Bounded join: a worker stuck inside libgit2 (corrupted packfile,
        // hung NFS) must not freeze app shutdown / repo switch.
        if let Some(h) = self.handle.take() {
            crate::platform::threading::try_timed_join(h, crate::platform::threading::REAP_TIMEOUT);
        }
    }
}

/// One status read plus the mtimes of what it listed.
pub(super) fn read(
    repo: &git2::Repository,
) -> anyhow::Result<(RepoSnapshot, HashMap<String, SystemTime>)> {
    let snapshot = load_snapshot(repo)?;
    let mtimes = repo
        .workdir()
        .map(|workdir| collect_mtimes(workdir, &snapshot))
        .unwrap_or_default();
    Ok((snapshot, mtimes))
}

pub enum SnapshotMsg {
    Ok(RepoSnapshot, HashMap<String, SystemTime>),
    Err(String),
}

/// Stat every file in `snapshot` against `repo_root` and return its mtime.
/// Files that cannot be stat'd (deleted between snapshot and stat) are
/// dropped; absence in the returned map removes them from `hot_table`.
/// Runs on the snapshot worker thread to keep filesystem syscalls off the
/// UI thread.
fn collect_mtimes(repo_root: &Path, snapshot: &RepoSnapshot) -> HashMap<String, SystemTime> {
    let mut out = HashMap::with_capacity(snapshot.files.len());
    for f in &snapshot.files {
        if let Ok(meta) = std::fs::metadata(repo_root.join(&f.path))
            && let Ok(mtime) = meta.modified()
        {
            out.insert(f.path.clone(), mtime);
        }
    }
    out
}

#[cfg(test)]
#[path = "snapshot_tests.rs"]
mod tests;
