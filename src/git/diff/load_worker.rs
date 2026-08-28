//! Conflated background loads for the TUI git views.

mod execute;
mod lifecycle;

use std::sync::{Arc, Condvar, Mutex, mpsc};
use std::thread::{self, JoinHandle};

use git2::{Oid, Repository};

use super::{ChangedFile, DiffHunk, LogDecorations, StatusKind};
use execute::execute;
use lifecycle::{InFlightPermit, finish_or_retire, reap_retired};

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
    handle: Option<JoinHandle<()>>,
}

impl GitLoadWorker {
    pub(crate) fn spawn() -> Self {
        reap_retired();
        let shared = Arc::new((Mutex::new(Pending::default()), Condvar::new()));
        let worker_shared = Arc::clone(&shared);
        let (reply_tx, replies) = mpsc::channel();
        let handle = thread::spawn(move || worker_loop(worker_shared, reply_tx));
        Self {
            shared,
            replies,
            handle: Some(handle),
        }
    }

    pub(crate) fn submit(&self, request: GitLoadRequest) {
        let (lock, wake) = &*self.shared;
        let mut pending = lock.lock().unwrap_or_else(|e| e.into_inner());
        if pending.stopped {
            return;
        }
        pending.replace(request);
        wake.notify_one();
    }

    pub(crate) fn cancel(&self, lane: LoadLane, generation: u64) {
        let mut pending = self.shared.0.lock().unwrap_or_else(|e| e.into_inner());
        pending.cancel(lane, generation);
    }

    pub(crate) fn try_recv(&self) -> Result<GitLoadReply, mpsc::TryRecvError> {
        self.replies.try_recv()
    }
}

impl Drop for GitLoadWorker {
    fn drop(&mut self) {
        let (lock, wake) = &*self.shared;
        lock.lock().unwrap_or_else(|e| e.into_inner()).stopped = true;
        wake.notify_one();
        if let Some(handle) = self.handle.take() {
            finish_or_retire(handle);
        }
    }
}

fn worker_loop(shared: Arc<(Mutex<Pending>, Condvar)>, replies: mpsc::Sender<GitLoadReply>) {
    let mut cached: Option<(String, Repository)> = None;
    loop {
        let request = {
            let (lock, wake) = &*shared;
            let mut pending = lock.lock().unwrap_or_else(|e| e.into_inner());
            while !pending.stopped && pending.requests.iter().all(Option::is_none) {
                pending = wake.wait(pending).unwrap_or_else(|e| e.into_inner());
            }
            if pending.stopped {
                return;
            }
            pending.take_next().expect("a pending request was observed")
        };

        if !shared
            .0
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .is_latest(&request)
        {
            continue;
        }
        let Some(_permit) = InFlightPermit::acquire(&request.repo, || {
            shared.0.lock().unwrap_or_else(|e| e.into_inner()).stopped
        }) else {
            return;
        };
        let result = execute(&request, &mut cached).map_err(|e| e.to_string());
        if replies.send(GitLoadReply { request, result }).is_err() {
            return;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(generation: u64, operation: GitLoadOperation) -> GitLoadRequest {
        GitLoadRequest {
            repo: "repo".into(),
            generation,
            operation,
        }
    }

    #[test]
    fn 같은_lane의_대기_요청은_최신_요청_하나로_합쳐진다() {
        let mut pending = Pending::default();
        for generation in 1..=100_000 {
            pending.replace(request(
                generation,
                GitLoadOperation::StatusDiff(format!("{generation}.rs")),
            ));
        }

        let latest = pending.take_next().unwrap();
        assert_eq!(latest.generation, 100_000);
        assert!(pending.take_next().is_none());
    }

    #[test]
    fn 서로_다른_lane의_요청은_서로를_덮어쓰지_않는다() {
        let mut pending = Pending::default();
        pending.replace(request(1, GitLoadOperation::StatusDiff("a.rs".into())));
        pending.replace(request(2, GitLoadOperation::WorkdirFile("a.rs".into())));

        assert!(pending.take_next().is_some());
        assert!(pending.take_next().is_some());
    }

    #[test]
    fn continuously_refilled_diff_lane_cannot_starve_other_lanes() {
        let mut pending = Pending::default();
        pending.replace(request(1, GitLoadOperation::StatusDiff("a.rs".into())));
        pending.replace(request(2, GitLoadOperation::WorkdirFile("a.rs".into())));
        pending.replace(request(3, GitLoadOperation::CommitFiles(Oid::ZERO_SHA1)));
        pending.replace(request(4, GitLoadOperation::Decorations));

        let mut lanes = Vec::new();
        for generation in 5..9 {
            let next = pending.take_next().unwrap();
            lanes.push(next.operation.lane());
            pending.replace(request(
                generation,
                GitLoadOperation::StatusDiff(format!("{generation}.rs")),
            ));
        }

        assert!(lanes.contains(&LoadLane::File));
        assert!(lanes.contains(&LoadLane::CommitFiles));
        assert!(lanes.contains(&LoadLane::Decorations));
    }
}
