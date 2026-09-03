use super::super::ViewerState;
use super::super::http_util::{json_error, json_response};
use super::lookup::{lookup_repo, redact};
use crate::web::common::http::RequestHead;

#[derive(serde::Deserialize)]
struct WriteFileRequest {
    /// The full new contents of the file.
    content: String,
    /// The blob oid the edit was based on — the version the client loaded.
    /// A mismatch with what is on disk now means the file moved underneath the
    /// edit, and the write is refused unless `force`.
    base_hash: String,
    /// Overwrite even when the on-disk version has moved on from `base_hash`.
    #[serde(default)]
    force: bool,
}

/// Overwrite a working-tree file with edited contents.
///
/// The one route that changes a repository *file* rather than viewer state. It
/// stays inside the same trust boundary as the terminal — an authenticated
/// user can already `echo > file` in the shell here — so it is a POST guarded
/// by the same Origin check and `SameSite=Strict` cookie as the other
/// mutations, and it runs the worktree gate `resolve_in_workdir` shares with
/// `/api/file` (no traversal, no symlinks, never the git directory). Only an
/// existing working-tree file is a target: the gate stats every component, so
/// a missing path is rejected, and a commit's version is read-only history.
///
/// Optimistic concurrency: `base_hash` is the blob oid the edit started from.
/// If the file on disk no longer hashes to it, the write is refused `409` with
/// the current oid so an edit cannot silently clobber a change made underneath
/// it; `force` is the client's "overwrite anyway".
pub(in crate::web::viewer::server) fn handle_write_file(
    head: &RequestHead,
    body: &str,
    state: &ViewerState,
) -> Vec<u8> {
    let entry = match lookup_repo(head, state) {
        Ok(entry) => entry,
        Err(response) => return response,
    };
    let Some(path) = head.query_param("path").filter(|p| !p.is_empty()) else {
        return json_error("400 Bad Request", "missing path parameter");
    };
    let request: WriteFileRequest = match serde_json::from_str(body) {
        Ok(request) => request,
        Err(_) => {
            return json_error(
                "400 Bad Request",
                "expected a JSON body with content and baseHash",
            );
        }
    };

    let repo = match super::super::handlers::open_repo(&entry.path) {
        Ok(repo) => repo,
        Err(err) => return redact("write open", &err),
    };
    let Some(workdir) = repo.workdir().map(std::path::Path::to_path_buf) else {
        return json_error("400 Bad Request", "bare repository");
    };
    let target = match crate::git::path::resolve_in_workdir(&workdir, &path) {
        Ok(target) => target,
        Err(err) => return redact("write resolve", &err),
    };

    // The version on disk right now, to compare against what the edit began
    // from before anything is written.
    let current = match std::fs::read(&target) {
        Ok(bytes) => bytes,
        Err(err) => return redact("write read", &anyhow::Error::new(err)),
    };
    let current_oid = match git2::Oid::hash_object(git2::ObjectType::Blob, &current) {
        Ok(oid) => oid.to_string(),
        Err(err) => return redact("write hash", &anyhow::Error::new(err)),
    };
    if current_oid != request.base_hash && !request.force {
        // The `current_oid` is a hex blob id, not attacker-shaped text, so it
        // is safe to interpolate into the fixed JSON here.
        return json_response(
            "409 Conflict",
            &format!("{{\"error\":\"stale\",\"currentHash\":\"{current_oid}\"}}"),
            &[],
        );
    }

    if let Err(err) = std::fs::write(&target, request.content.as_bytes()) {
        return redact("write", &anyhow::Error::new(err));
    }
    let new_oid = match git2::Oid::hash_object(git2::ObjectType::Blob, request.content.as_bytes()) {
        Ok(oid) => oid.to_string(),
        Err(err) => return redact("write hash", &anyhow::Error::new(err)),
    };
    super::encode_response(
        serde_json::json!({ "hash": new_oid }),
        "could not encode the write result",
    )
}
