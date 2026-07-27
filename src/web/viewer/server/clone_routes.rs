//! Start and poll clones of a remote repository.
//!
//! Split from `mutations.rs` because a clone is the one mutation that does not
//! finish inside its request: `POST /api/clone` validates, spawns, and answers
//! with a job id; `GET /api/clone?job=<id>` reports on it until it is done.

use super::ViewerState;
use super::http_util::{json_error, json_response};
use crate::git::clone::{run_clone, validate_clone_url};
use crate::web::common::http::RequestHead;
use crate::web::viewer::clone_jobs::CloneState;
use crate::web::viewer::dto::Envelope;
use std::path::PathBuf;
use std::sync::Arc;

#[derive(serde::Deserialize)]
struct CloneRequest {
    /// The directory to clone into — the one the picker is showing.
    path: String,
    /// The remote address. Validated by `git::clone::validate_clone_url`.
    url: String,
}

/// Start a clone under the browsed directory and return its job id.
///
/// The destination is derived from the URL, never supplied by the client, and
/// is a single plain segment by construction — so, as with `mkdir`, the clone
/// can only land directly under the canonicalized parent. The URL's scheme is
/// checked before `git` sees it: `ext::` executes a command, so an unfiltered
/// URL here would be remote code execution on the server.
pub(super) fn handle_clone(body: &str, state: &Arc<ViewerState>) -> Vec<u8> {
    let request: CloneRequest = match serde_json::from_str(body) {
        Ok(request) => request,
        Err(_) => {
            return json_error(
                "400 Bad Request",
                "expected a JSON body with a path and a url",
            );
        }
    };
    let url = request.url.trim().to_string();
    let name = match validate_clone_url(&url) {
        Ok(name) => name,
        Err(err) => return json_error("400 Bad Request", err.message()),
    };
    let parent = crate::platform::paths::expand_tilde(request.path.trim());
    if !parent.is_dir() {
        return json_error("400 Bad Request", "no such directory");
    }
    // Canonicalize first so a symlink in the supplied path cannot redirect the
    // join, matching `handle_mkdir`.
    let base = match parent.canonicalize() {
        Ok(base) => base,
        Err(_) => return json_error("400 Bad Request", "no such directory"),
    };
    let dest = base.join(&name);
    if dest.exists() {
        return json_error(
            "409 Conflict",
            "a folder with that repository's name already exists here",
        );
    }
    // One at a time. Concurrent clones would let a single client fill the
    // server's disk from several remotes at once, and the picker only ever
    // shows one in progress.
    if state.clones.any_running() {
        return json_error("409 Conflict", "a clone is already running");
    }

    let id = state.clones.start();
    let worker = Arc::clone(state);
    if let Err(err) = std::thread::Builder::new()
        .name("nightcrow-viewer-clone".to_string())
        .spawn(move || run_and_record(&worker, id, &url, dest))
    {
        state.clones.finish(
            id,
            CloneState::Failed("could not start the clone".to_string()),
        );
        tracing::warn!(error = %err, "clone thread failed to start");
        return json_error("500 Internal Server Error", "could not start the clone");
    }
    encode(serde_json::json!({ "job": id, "name": name }))
}

fn run_and_record(state: &ViewerState, id: u64, url: &str, dest: PathBuf) {
    let result = run_clone(url, &dest);
    let outcome = match result {
        Ok(()) => CloneState::Done(dest.to_string_lossy().into_owned()),
        Err(err) => {
            // git's message names the real problem ("repository not found",
            // "permission denied"), which is exactly what the user must act on.
            // It is the remote's words about a URL the user typed, not server
            // internals, so it is shown rather than redacted.
            tracing::info!(error = %err, "clone failed");
            CloneState::Failed(err.to_string())
        }
    };
    state.clones.finish(id, outcome);
}

/// Report on a job. An id that was never handed out — or one already evicted
/// after the client read it — is a 404 rather than a silent "running".
pub(super) fn handle_clone_status(head: &RequestHead, state: &ViewerState) -> Vec<u8> {
    let Some(id) = head
        .query_param("job")
        .and_then(|raw| raw.parse::<u64>().ok())
    else {
        return json_error("400 Bad Request", "a job id is required");
    };
    let Some(job) = state.clones.get(id) else {
        return json_error("404 Not Found", "no such clone");
    };
    let payload = match job {
        CloneState::Running => serde_json::json!({ "state": "running" }),
        CloneState::Done(path) => serde_json::json!({ "state": "done", "path": path }),
        CloneState::Failed(message) => {
            serde_json::json!({ "state": "failed", "message": message })
        }
    };
    encode(payload)
}

fn encode(payload: serde_json::Value) -> Vec<u8> {
    match serde_json::to_string(&Envelope::new(payload)) {
        Ok(json) => json_response("200 OK", &json, &[]),
        Err(_) => json_error("500 Internal Server Error", "could not encode the clone"),
    }
}
