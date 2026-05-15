use crate::git::diff::{RepoSnapshot, load_snapshot};
use std::collections::HashMap;
use std::path::Path;
use std::sync::mpsc::{self, Receiver, SyncSender};
use std::thread;
use std::time::{Duration, SystemTime};

/// Owns the receiver and stop channel for the background snapshot thread.
/// Dropping the struct signals the worker to exit and joins it before
/// returning, so a repo switch cannot leave the old-repo worker holding a
/// `git2::Repository` after the new channel is in place.
pub struct SnapshotChannel {
    rx: Receiver<SnapshotMsg>,
    // Held in an Option so `Drop` can release it before joining the
    // worker. Dropping the sender unblocks the worker's `recv_timeout`
    // immediately rather than letting it sleep up to the snapshot
    // interval.
    stop_tx: Option<SyncSender<()>>,
    // None in test fixtures that construct an inert channel via
    // `from_endpoints` (no real worker to join).
    handle: Option<thread::JoinHandle<()>>,
}

/// Reopen the cached `git2::Repository` handle every N ticks so we observe
/// out-of-band repo changes (e.g. `git gc`, packfile rewrites, worktree
/// moves) that the cached handle would otherwise serve stale. ~30 s at the
/// current 1 s tick is cheap and predictable.
const REOPEN_REPO_EVERY_TICKS: u32 = 30;

impl SnapshotChannel {
    pub fn spawn(repo_path: &str) -> Self {
        let (tx, rx) = mpsc::channel::<SnapshotMsg>();
        let (stop_tx, stop_rx) = mpsc::sync_channel::<()>(0);
        let path = repo_path.to_string();
        let handle = thread::spawn(move || {
            // Cache the Repository handle to avoid a fresh `discover` walk
            // every tick, but drop it periodically (and on any load error)
            // so external repo mutations cannot leave us serving stale state.
            let mut repo: Option<git2::Repository> = None;
            let mut ticks_since_open: u32 = 0;
            loop {
                if ticks_since_open >= REOPEN_REPO_EVERY_TICKS {
                    repo = None;
                }
                if repo.is_none() {
                    match git2::Repository::discover(&path) {
                        Ok(r) => {
                            repo = Some(r);
                            ticks_since_open = 0;
                        }
                        Err(e) => {
                            let msg = SnapshotMsg::Err(format!("not a git repository: {e}"));
                            if tx.send(msg).is_err() {
                                break;
                            }
                            match stop_rx.recv_timeout(Duration::from_millis(1000)) {
                                Ok(()) | Err(mpsc::RecvTimeoutError::Disconnected) => break,
                                Err(mpsc::RecvTimeoutError::Timeout) => {}
                            }
                            continue;
                        }
                    }
                }
                let r = repo.as_ref().expect("repo just opened");
                let msg = match load_snapshot(r) {
                    Ok(s) => {
                        let mtimes = r
                            .workdir()
                            .map(|w| collect_mtimes(w, &s))
                            .unwrap_or_default();
                        SnapshotMsg::Ok(s, mtimes)
                    }
                    Err(e) => {
                        // Drop the handle: the next tick will re-discover.
                        // This covers the case where the repo was relocated
                        // or its internal state became inconsistent. Reset
                        // the reopen counter alongside the handle so the
                        // next successful open restarts the cycle cleanly
                        // instead of carrying over a stale tick count.
                        repo = None;
                        ticks_since_open = 0;
                        SnapshotMsg::Err(e.to_string())
                    }
                };
                ticks_since_open += 1;
                if tx.send(msg).is_err() {
                    break;
                }
                match stop_rx.recv_timeout(Duration::from_millis(1000)) {
                    Ok(()) | Err(mpsc::RecvTimeoutError::Disconnected) => break,
                    Err(mpsc::RecvTimeoutError::Timeout) => {}
                }
            }
        });
        Self {
            rx,
            stop_tx: Some(stop_tx),
            handle: Some(handle),
        }
    }

    pub fn try_recv(&self) -> Result<SnapshotMsg, mpsc::TryRecvError> {
        self.rx.try_recv()
    }

    /// Build a `SnapshotChannel` from externally provided endpoints. Lets
    /// tests construct an inert channel (no worker thread) so they can
    /// inject snapshots directly via `ingest_snapshot` instead of booting
    /// the background discoverer.
    #[cfg(test)]
    pub(crate) fn from_endpoints(rx: Receiver<SnapshotMsg>, stop_tx: SyncSender<()>) -> Self {
        Self {
            rx,
            stop_tx: Some(stop_tx),
            handle: None,
        }
    }
}

impl Drop for SnapshotChannel {
    fn drop(&mut self) {
        // Release the stop sender first: the worker's `recv_timeout`
        // observes `Disconnected` immediately rather than sleeping out
        // the remaining tick interval.
        drop(self.stop_tx.take());
        // Wait for the worker to finish its current `load_snapshot` so a
        // `change_repo` doesn't leave the old-repo worker running with a
        // live `git2::Repository` after the new channel is installed. A
        // panic in the worker is logged rather than propagated — the
        // UI thread is in the middle of a drop and cannot meaningfully
        // recover.
        if let Some(h) = self.handle.take()
            && let Err(e) = h.join()
        {
            tracing::warn!(?e, "snapshot worker panicked during shutdown");
        }
    }
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
