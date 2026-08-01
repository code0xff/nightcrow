use super::super::ViewerState;
use super::super::http_util::json_error;
use super::super::mutations::{lookup_repo, redact};
use super::http::required_path;
use crate::session::catalog::RepoEntry;
use anyhow::{Context, Result};

/// Look the repository up, validate any `path` parameter, then run `body`.
///
/// Validation happens here rather than in each handler so no route can forget
/// it. A traversal path is refused uniformly, and never echoed back.
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

/// Variant of [`with_repo`] for a path inside a historical git object.
///
/// A deleted commit path cannot be resolved in the current worktree, so this
/// validates its syntax without statting it.
pub(in crate::web::viewer::server) fn with_repo_commit_path(
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
