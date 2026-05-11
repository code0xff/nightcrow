use crate::git::diff::{RepoSnapshot, load_snapshot};
use std::collections::HashMap;
use std::path::Path;
use std::sync::mpsc::{self, Receiver, SyncSender};
use std::thread;
use std::time::{Duration, SystemTime};

/// Owns the receiver and stop channel for the background snapshot thread.
/// Dropping the struct (and its `_stop_tx`) signals the thread to exit.
pub struct SnapshotChannel {
    pub(crate) rx: Receiver<SnapshotMsg>,
    // Held only for its drop side-effect: dropping the sender unblocks
    // the worker's recv_timeout so it can observe the disconnect.
    pub(crate) _stop_tx: SyncSender<()>,
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
        thread::spawn(move || {
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
                        // or its internal state became inconsistent.
                        repo = None;
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
            _stop_tx: stop_tx,
        }
    }

    pub fn try_recv(&self) -> Result<SnapshotMsg, mpsc::TryRecvError> {
        self.rx.try_recv()
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
