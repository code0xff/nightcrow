use super::super::ViewerState;
use super::super::http_util::json_error;
use crate::session::catalog::RepoEntry;
use crate::web::common::http::RequestHead;
use anyhow::Result;
use std::sync::Arc;

/// Resolve the `repo` parameter to an entry, or produce the 404 response.
pub(in crate::web::viewer::server) fn lookup_repo(
    head: &RequestHead,
    state: &ViewerState,
) -> Result<Arc<RepoEntry>, Vec<u8>> {
    let id = head
        .query_param("repo")
        .ok_or_else(|| json_error("400 Bad Request", "missing repo parameter"))?;
    state
        .session
        .catalog()
        .get(&id)
        .ok_or_else(|| json_error("404 Not Found", "unknown repository"))
}

/// Map an internal error to a fixed public message, logging the detail: git
/// and I/O errors may name absolute paths, symlink targets, and file sizes.
pub(in crate::web::viewer::server) fn redact(context: &str, err: &anyhow::Error) -> Vec<u8> {
    tracing::debug!(%err, context, "viewer: request failed");
    json_error("400 Bad Request", "request could not be served")
}
