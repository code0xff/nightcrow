//! Conflated background loads for the TUI git views.

use std::sync::{Arc, Condvar, Mutex, mpsc};
use std::thread::{self, JoinHandle};

use git2::{Oid, Repository};

use super::{
    ChangedFile, DiffHunk, LogDecorations, StatusKind, load_commit_diff, load_commit_file_blob,
    load_commit_file_diff, load_commit_files, load_file_diff, load_log_decorations,
    load_workdir_file,
};
use crate::platform::threading::{REAP_TIMEOUT, try_timed_join};

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
    stopped: bool,
}

impl Default for Pending {
    fn default() -> Self {
        Self {
            requests: std::array::from_fn(|_| None),
            latest: [0; LoadLane::COUNT],
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
        self.requests.iter_mut().find_map(Option::take)
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
            try_timed_join(handle, REAP_TIMEOUT);
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
        let result = execute(&request, &mut cached).map_err(|e| e.to_string());
        if replies.send(GitLoadReply { request, result }).is_err() {
            return;
        }
    }
}

fn execute(
    request: &GitLoadRequest,
    cached: &mut Option<(String, Repository)>,
) -> anyhow::Result<GitLoadPayload> {
    if cached
        .as_ref()
        .is_none_or(|(path, _)| path != &request.repo)
    {
        let repo = Repository::discover(&request.repo)
            .map_err(|e| anyhow::anyhow!(crate::git::format_discover_error(&e)))?;
        *cached = Some((request.repo.clone(), repo));
    }
    let repo = &cached.as_ref().expect("repository was opened").1;
    let result = match &request.operation {
        GitLoadOperation::StatusDiff(path) => load_file_diff(repo, path).map(GitLoadPayload::Diff),
        GitLoadOperation::CommitDiff(oid) => load_commit_diff(repo, *oid).map(GitLoadPayload::Diff),
        GitLoadOperation::CommitFileDiff { oid, path } => {
            load_commit_file_diff(repo, *oid, path).map(GitLoadPayload::Diff)
        }
        GitLoadOperation::WorkdirFile(path) => {
            load_workdir_file(repo, path).map(GitLoadPayload::File)
        }
        GitLoadOperation::CommitFile { oid, path, status } => {
            load_commit_file_blob(repo, *oid, path, *status).map(GitLoadPayload::File)
        }
        GitLoadOperation::CommitFiles(oid) => {
            load_commit_files(repo, *oid).map(GitLoadPayload::CommitFiles)
        }
        GitLoadOperation::Decorations => {
            load_log_decorations(repo).map(GitLoadPayload::Decorations)
        }
    };
    if result.as_ref().err().is_some_and(is_repository_error) {
        *cached = None;
    }
    result
}

fn is_repository_error(error: &anyhow::Error) -> bool {
    error
        .downcast_ref::<git2::Error>()
        .is_some_and(|git_error| {
            matches!(
                git_error.class(),
                git2::ErrorClass::Os | git2::ErrorClass::Repository
            )
        })
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
}
