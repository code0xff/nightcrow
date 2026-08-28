mod commit_log;
mod conflict;
mod diff_load;
mod file_load;
mod load_worker;
mod refs;
mod snapshot;
mod types;

#[cfg(test)]
pub(crate) use commit_log::is_empty_head;
pub use commit_log::{
    head_commit_oid, load_commit_log, load_commit_log_from, load_commit_log_page,
};
pub use diff_load::{
    load_commit_diff, load_commit_file_diff, load_commit_files, load_file_diff,
    parse_hunk_new_start,
};
pub use file_load::{load_commit_file, load_commit_file_blob, load_workdir_file};
pub(crate) use load_worker::{
    GitLoadOperation, GitLoadPayload, GitLoadReply, GitLoadRequest, GitLoadWorker, LoadLane,
};
pub use refs::{LogDecorations, RefKind, RefLabel, load_log_decorations};
pub use snapshot::load_snapshot;
#[cfg(test)]
pub use types::DiffLine;
pub use types::{
    ChangedFile, CommitEntry, DiffHunk, LineKind, RepoSnapshot, StatusKind, TrackingStatus,
};

#[cfg(test)]
mod tests;
