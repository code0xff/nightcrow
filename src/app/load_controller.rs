use crate::git::diff::{GitLoadOperation, GitLoadRequest, GitLoadWorker, LoadLane};
use crate::ui::file_view::FileViewKey;

#[derive(Clone)]
pub(crate) enum DiffTarget {
    Status(String),
    Commit(git2::Oid),
    CommitFile { oid: git2::Oid, path: String },
}

impl DiffTarget {
    fn operation(&self) -> GitLoadOperation {
        match self {
            Self::Status(path) => GitLoadOperation::StatusDiff(path.clone()),
            Self::Commit(oid) => GitLoadOperation::CommitDiff(*oid),
            Self::CommitFile { oid, path } => GitLoadOperation::CommitFileDiff {
                oid: *oid,
                path: path.clone(),
            },
        }
    }
}

#[derive(Clone)]
pub(crate) enum DiffLoadMode {
    Reset,
    KeepScroll(usize),
    ResetWithTitle(String),
}

#[derive(Clone)]
pub(crate) struct DiffIntent {
    pub(crate) generation: u64,
    pub(crate) repo: String,
    pub(crate) target: DiffTarget,
    pub(crate) mode: DiffLoadMode,
    pub(crate) restore_scroll: Option<usize>,
}

#[derive(Clone)]
pub(crate) struct FileIntent {
    pub(crate) generation: u64,
    pub(crate) repo: String,
    pub(crate) key: FileViewKey,
    pub(crate) anchor: Option<usize>,
}

#[derive(Clone)]
pub(crate) struct CommitFilesIntent {
    pub(crate) generation: u64,
    pub(crate) repo: String,
    pub(crate) oid: git2::Oid,
    pub(crate) title: String,
}

#[derive(Clone)]
pub(crate) struct DecorationsIntent {
    pub(crate) generation: u64,
    pub(crate) repo: String,
    pub(crate) fingerprint: u64,
}

pub(crate) struct LoadController {
    pub(crate) worker: GitLoadWorker,
    next_generation: u64,
    pub(crate) diff: Option<DiffIntent>,
    pub(crate) file: Option<FileIntent>,
    pub(crate) commit_files: Option<CommitFilesIntent>,
    pub(crate) decorations: Option<DecorationsIntent>,
}

impl LoadController {
    pub(crate) fn new() -> Self {
        Self {
            worker: GitLoadWorker::spawn(),
            next_generation: 0,
            diff: None,
            file: None,
            commit_files: None,
            decorations: None,
        }
    }

    fn generation(&mut self) -> u64 {
        self.next_generation = self.next_generation.wrapping_add(1);
        self.next_generation
    }

    pub(crate) fn request_diff(&mut self, repo: &str, target: DiffTarget, mode: DiffLoadMode) {
        let generation = self.generation();
        let operation = target.operation();
        self.diff = Some(DiffIntent {
            generation,
            repo: repo.to_string(),
            target,
            mode,
            restore_scroll: None,
        });
        self.worker.submit(GitLoadRequest {
            repo: repo.to_string(),
            generation,
            operation,
        });
    }

    pub(crate) fn request_file(&mut self, repo: &str, key: FileViewKey, anchor: Option<usize>) {
        let generation = self.generation();
        let operation = match &key {
            FileViewKey::Status(path) => GitLoadOperation::WorkdirFile(path.clone()),
            FileViewKey::Commit { oid, path, status } => GitLoadOperation::CommitFile {
                oid: *oid,
                path: path.clone(),
                status: *status,
            },
        };
        self.file = Some(FileIntent {
            generation,
            repo: repo.to_string(),
            key,
            anchor,
        });
        self.worker.submit(GitLoadRequest {
            repo: repo.to_string(),
            generation,
            operation,
        });
    }

    pub(crate) fn request_commit_files(&mut self, repo: &str, oid: git2::Oid, title: String) {
        let generation = self.generation();
        self.commit_files = Some(CommitFilesIntent {
            generation,
            repo: repo.to_string(),
            oid,
            title,
        });
        self.worker.submit(GitLoadRequest {
            repo: repo.to_string(),
            generation,
            operation: GitLoadOperation::CommitFiles(oid),
        });
    }

    pub(crate) fn request_decorations(&mut self, repo: &str, fingerprint: u64) {
        let generation = self.generation();
        self.decorations = Some(DecorationsIntent {
            generation,
            repo: repo.to_string(),
            fingerprint,
        });
        self.worker.submit(GitLoadRequest {
            repo: repo.to_string(),
            generation,
            operation: GitLoadOperation::Decorations,
        });
    }

    pub(crate) fn file_generation(&self) -> Option<u64> {
        self.file.as_ref().map(|intent| intent.generation)
    }

    pub(crate) fn restore_diff_scroll(&mut self, scroll: usize) {
        if let Some(intent) = self.diff.as_mut() {
            intent.restore_scroll = Some(scroll);
        }
    }

    pub(crate) fn cancel_diff(&mut self) {
        let generation = self.generation();
        self.diff = None;
        self.worker.cancel(LoadLane::Diff, generation);
    }

    pub(crate) fn cancel_file(&mut self) {
        let generation = self.generation();
        self.file = None;
        self.worker.cancel(LoadLane::File, generation);
    }

    pub(crate) fn cancel_commit_files(&mut self) {
        let generation = self.generation();
        self.commit_files = None;
        self.worker.cancel(LoadLane::CommitFiles, generation);
    }
}
