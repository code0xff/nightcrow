mod commit_log;
mod diff_load;
mod snapshot;
mod types;

pub use commit_log::{
    head_commit_oid, load_commit_log, load_commit_log_from, load_commit_log_page,
};
#[cfg(test)]
pub(crate) use commit_log::is_empty_head;
pub use diff_load::{
    load_commit_diff, load_commit_file_blob, load_commit_file_diff,
    load_commit_files, load_file_diff, load_workdir_file, parse_hunk_new_start,
};
pub use snapshot::load_snapshot;
pub use types::{
    ChangedFile, CommitEntry, DiffHunk, LineKind, RepoSnapshot, StatusKind,
    TrackingStatus,
};
#[cfg(test)]
pub use types::DiffLine;

#[cfg(test)]
mod tests;