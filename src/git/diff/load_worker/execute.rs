use git2::Repository;

use super::super::{
    load_commit_diff, load_commit_file_blob, load_commit_file_diff, load_commit_files,
    load_file_diff, load_log_decorations, load_workdir_file,
};
use super::{GitLoadOperation, GitLoadPayload, GitLoadRequest};

pub(super) fn execute(
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
