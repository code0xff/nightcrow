use super::super::ViewerState;
use super::super::http_util::json_error;
use super::super::mutations::{lookup_repo, redact};
use super::http::required_path;
use crate::session::catalog::RepoEntry;
use anyhow::{Context, Result};

/// Look the repository up, validate any `path` parameter, then run `body`.
/// Validation happens here rather than in each handler so no route can
/// forget it; a traversal path is refused uniformly and never echoed back.
pub(in crate::web::viewer::server) fn with_repo(
    head: &crate::web::common::http::RequestHead,
    state: &ViewerState,
    body: impl FnOnce(&RepoEntry) -> Result<Vec<u8>>,
) -> Vec<u8> {
    let entry = match lookup_repo(head, state) {
        Ok(entry) => entry,
        Err(response) => return response,
    };
    // An absent or empty `path` means "the repository root" for the routes that
    // accept one; anything else has to survive the gate.
    if let Some(path) = head.query_param("path").filter(|p| !p.is_empty())
        && let Err(err) =
            crate::git::path::resolve_in_workdir(std::path::Path::new(&entry.path), &path)
    {
        tracing::debug!(%err, route = %head.path, "viewer: rejected path parameter");
        return json_error("400 Bad Request", "invalid path");
    }
    match body(&entry) {
        Ok(response) => response,
        Err(err) => redact(&head.path, &err),
    }
}

/// Serve a route whose `path` is handed to *git*, not opened.
///
/// Validated as a pathspec — traversal, `.git`, NUL — and no further. The
/// stricter [`with_repo`] adds refusing symlinks and requiring the path to
/// exist, which are what protect a file this process is about to open; git
/// reads a symlink as a blob holding the target's name, never the target's
/// contents, and a path that is gone is exactly what a deletion diff is
/// about. Named for what the path is *for* rather than where it came from: a
/// commit's file and a deleted worktree file need the same rule for the same
/// reason — neither is on disk to be resolved — and calling it "commit" sent
/// the second one to the gate that turned it into a 400.
pub(in crate::web::viewer::server) fn with_repo_git_path(
    head: &crate::web::common::http::RequestHead,
    state: &ViewerState,
    body: impl FnOnce(&RepoEntry, &str) -> Result<Vec<u8>>,
) -> Vec<u8> {
    let entry = match lookup_repo(head, state) {
        Ok(entry) => entry,
        Err(response) => return response,
    };
    let path = match required_path(head) {
        Ok(path) => path,
        Err(err) => return redact(&head.path, &err),
    };
    if let Err(err) = crate::git::path::validate_commit_path(&path) {
        tracing::debug!(%err, route = %head.path, "viewer: rejected historical path parameter");
        return json_error("400 Bad Request", "invalid path");
    }
    match body(&entry, &path) {
        Ok(response) => response,
        Err(err) => redact(&head.path, &err),
    }
}

pub(in crate::web::viewer::server) fn open_repo(path: &str) -> Result<git2::Repository> {
    git2::Repository::discover(path).context("failed to open repository")
}
