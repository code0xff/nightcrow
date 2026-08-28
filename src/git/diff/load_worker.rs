//! Conflated background loads for the TUI git views.

mod execute;
mod lifecycle;
mod retry;
mod runtime;

use std::sync::{Arc, Condvar, Mutex, mpsc};

use git2::Oid;

use super::{ChangedFile, DiffHunk, LogDecorations, StatusKind};
use runtime::WorkerThread;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LoadLane {
    Diff,
    File,
    CommitFiles,
    Decorations,
}

impl LoadLane {
    const COUNT: usize = 4;

    fn index(self) -> usize {
        match self {
            Self::Diff => 0,
            Self::File => 1,
            Self::CommitFiles => 2,
            Self::Decorations => 3,
        }
    }
}

#[derive(Clone)]
pub(crate) enum GitLoadOperation {
    StatusDiff(String),
    CommitDiff(Oid),
    CommitFileDiff {
        oid: Oid,
        path: String,
    },
    WorkdirFile(String),
    CommitFile {
        oid: Oid,
        path: String,
        status: StatusKind,
    },
    CommitFiles(Oid),
    Decorations,
}

impl GitLoadOperation {
    pub(crate) fn lane(&self) -> LoadLane {
        match self {
            Self::StatusDiff(_) | Self::CommitDiff(_) | Self::CommitFileDiff { .. } => {
                LoadLane::Diff
            }
            Self::WorkdirFile(_) | Self::CommitFile { .. } => LoadLane::File,
            Self::CommitFiles(_) => LoadLane::CommitFiles,
            Self::Decorations => LoadLane::Decorations,
        }
    }
}

#[derive(Clone)]
pub(crate) struct GitLoadRequest {
    pub(crate) repo: String,
    pub(crate) generation: u64,
    pub(crate) operation: GitLoadOperation,
}

pub(crate) enum GitLoadPayload {
    Diff(Vec<DiffHunk>),
    File(String),
    CommitFiles(Vec<ChangedFile>),
    Decorations(LogDecorations),
}

pub(crate) struct GitLoadReply {
    pub(crate) request: GitLoadRequest,
    pub(crate) result: Result<GitLoadPayload, String>,
}

struct Pending {
    requests: [Option<GitLoadRequest>; LoadLane::COUNT],
    latest: [u64; LoadLane::COUNT],
    next_lane: usize,
    stopped: bool,
}

impl Default for Pending {
    fn default() -> Self {
        Self {
            requests: std::array::from_fn(|_| None),
            latest: [0; LoadLane::COUNT],
            next_lane: 0,
            stopped: false,
        }
    }
}

impl Pending {
    fn replace(&mut self, request: GitLoadRequest) {
        let index = request.operation.lane().index();
        self.latest[index] = request.generation;
        self.requests[index] = Some(request);
    }

    fn take_next(&mut self) -> Option<GitLoadRequest> {
        for offset in 0..LoadLane::COUNT {
            let index = (self.next_lane + offset) % LoadLane::COUNT;
            if let Some(request) = self.requests[index].take() {
                self.next_lane = (index + 1) % LoadLane::COUNT;
                return Some(request);
            }
        }
        None
    }

    fn cancel(&mut self, lane: LoadLane, generation: u64) {
        let index = lane.index();
        self.latest[index] = generation;
        self.requests[index] = None;
    }

    fn is_latest(&self, request: &GitLoadRequest) -> bool {
        self.latest[request.operation.lane().index()] == request.generation
    }
}

pub(crate) struct GitLoadWorker {
    shared: Arc<(Mutex<Pending>, Condvar)>,
    replies: mpsc::Receiver<GitLoadReply>,
    worker: Mutex<WorkerThread>,
}

impl GitLoadWorker {
    pub(crate) fn spawn() -> Self {
        Self::new(WorkerThread::new)
    }

    fn new(make_worker: impl FnOnce(mpsc::Sender<GitLoadReply>) -> WorkerThread) -> Self {
        let shared = Arc::new((Mutex::new(Pending::default()), Condvar::new()));
        let (reply_tx, replies) = mpsc::channel();
        let worker = Self {
            shared,
            replies,
            worker: Mutex::new(make_worker(reply_tx)),
        };
        worker.ensure_started();
        worker
    }

    pub(crate) fn submit(&self, request: GitLoadRequest) {
        let (lock, wake) = &*self.shared;
        let mut pending = lock.lock().unwrap_or_else(|e| e.into_inner());
        if pending.stopped {
            return;
        }
        pending.replace(request);
        wake.notify_one();
        drop(pending);
        self.ensure_started();
    }

    pub(crate) fn cancel(&self, lane: LoadLane, generation: u64) {
        let mut pending = self.shared.0.lock().unwrap_or_else(|e| e.into_inner());
        pending.cancel(lane, generation);
    }

    pub(crate) fn try_recv(&self) -> Result<GitLoadReply, mpsc::TryRecvError> {
        self.ensure_started();
        self.replies.try_recv()
    }

    fn ensure_started(&self) {
        let mut worker = self.worker.lock().unwrap_or_else(|e| e.into_inner());
        worker.ensure_started(Arc::clone(&self.shared));
    }
}

impl Drop for GitLoadWorker {
    fn drop(&mut self) {
        let (lock, wake) = &*self.shared;
        lock.lock().unwrap_or_else(|e| e.into_inner()).stopped = true;
        wake.notify_one();
        let worker = self.worker.get_mut().unwrap_or_else(|e| e.into_inner());
        worker.finish();
    }
}

#[cfg(test)]
mod tests;
