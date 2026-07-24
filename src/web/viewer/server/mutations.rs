use super::http_util::{json_error, json_response};
use super::ViewerState;
use crate::web::common::http::RequestHead;
use crate::web::viewer::catalog::{AddOutcome, RepoEntry};
use crate::web::viewer::dto::Envelope;
use anyhow::Result;
use std::sync::Arc;

#[derive(serde::Deserialize)]
struct OpenRequest {
    path: String,
}

#[derive(serde::Deserialize)]
struct PrefsRequest {
    /// Each preference is optional so one write touches one setting and leaves
    /// the rest as they are; a body naming none is rejected rather than treated
    /// as a silent no-op.
    accent: Option<usize>,
    sidebar_width: Option<u32>,
}

/// Store one or more viewer preferences and echo back the full stored set.
///
/// Each value is wrapped or clamped into range rather than rejected — an accent
/// index past the end of the cycle gets a colour back (as the TUI does with
/// `Accent::from_index`), and a width past the bounds gets a usable split back —
/// so a client that drifts out of range self-corrects from the response.
pub(super) fn handle_set_prefs(body: &str, state: &ViewerState) -> Vec<u8> {
    let request: PrefsRequest = match serde_json::from_str(body) {
        Ok(request) => request,
        Err(_) => {
            return json_error(
                "400 Bad Request",
                "expected a JSON body with a preference to store",
            );
        }
    };
    if request.accent.is_none() && request.sidebar_width.is_none() {
        return json_error("400 Bad Request", "no known preference in the body");
    }
    // One locked write for whatever the body carried, so a request naming both
    // preferences lands atomically rather than as two racing updates.
    let stored = state.prefs.update(request.accent, request.sidebar_width);
    match serde_json::to_string(&Envelope::new(serde_json::json!({
        "accent": stored.accent,
        "sidebar_width": stored.sidebar_width,
    }))) {
        Ok(json) => json_response("200 OK", &json, &[]),
        Err(_) => json_error("500 Internal Server Error", "could not encode preferences"),
    }
}

/// Open a repository from the browser and add it to the served catalog.
///
/// The path is user-supplied but the response is public, so a bad path yields a
/// generic message rather than echoing what was tried.
pub(super) fn handle_open_repo(body: &str, state: &ViewerState) -> Vec<u8> {
    let request: OpenRequest = match serde_json::from_str(body) {
        Ok(request) => request,
        Err(_) => return json_error("400 Bad Request", "expected a JSON body with a path"),
    };
    let raw = request.path.trim();
    if raw.is_empty() {
        return json_error("400 Bad Request", "a path is required");
    }
    let expanded = crate::util::expand_tilde(raw);
    // is_dir() follows symlinks and is false for a missing path — either way it
    // cannot be served.
    if !expanded.is_dir() {
        return json_error("400 Bad Request", "no such directory");
    }
    let resolved = crate::git::resolve_repo_path(&expanded)
        .to_string_lossy()
        .into_owned();

    match state.catalog.add_path(resolved, crate::workspace::MAX_PROJECTS) {
        AddOutcome::Added(repo) => {
            persist_workspace(state);
            match serde_json::to_string(&Envelope::new(serde_json::json!({ "repo": repo }))) {
                Ok(json) => json_response("200 OK", &json, &[]),
                Err(_) => json_error("500 Internal Server Error", "could not encode repository"),
            }
        }
        AddOutcome::TooMany => json_error(
            "409 Conflict",
            "the maximum number of repositories is already open",
        ),
    }
}

#[derive(serde::Deserialize)]
struct MkdirRequest {
    /// The directory to create the new folder inside — the one the picker is
    /// currently showing.
    path: String,
    /// The new folder's name. Must be a single plain path segment.
    name: String,
}

/// Create a new folder inside a directory the picker is browsing.
///
/// The parent is confined only as much as `browse` is (any directory an
/// authenticated user can already reach), but `name` is held to a single plain
/// segment: separators, `..`, a leading `.` (which also rules out `.git` and the
/// hidden entries the picker never lists), and NUL are all rejected. Combined
/// with canonicalizing the parent first, the created folder can only ever land
/// directly under the browsed directory, never escape it via a symlink or `..`.
pub(super) fn handle_mkdir(body: &str) -> Vec<u8> {
    let request: MkdirRequest = match serde_json::from_str(body) {
        Ok(request) => request,
        Err(_) => {
            return json_error(
                "400 Bad Request",
                "expected a JSON body with a path and a name",
            );
        }
    };
    let name = request.name.trim();
    if name.is_empty() {
        return json_error("400 Bad Request", "a folder name is required");
    }
    // A single plain segment only. This — not the parent — is what keeps the
    // create confined: no traversal, no separators, no hidden/.git, no NUL.
    if name.starts_with('.') || name.contains('/') || name.contains('\\') || name.contains('\0') {
        return json_error("400 Bad Request", "invalid folder name");
    }
    let parent = crate::util::expand_tilde(request.path.trim());
    // is_dir() follows symlinks and is false for a missing path — the same gate
    // `open` uses for the directory it is handed.
    if !parent.is_dir() {
        return json_error("400 Bad Request", "no such directory");
    }
    // Canonicalize first so a symlink in the supplied path cannot redirect the
    // join; the validated single-segment name then stays under the real parent.
    let base = match parent.canonicalize() {
        Ok(base) => base,
        Err(err) => return redact("mkdir canonicalize", &anyhow::Error::new(err)),
    };
    let target = base.join(name);
    match std::fs::create_dir(&target) {
        Ok(()) => {
            let path = target.to_string_lossy().into_owned();
            match serde_json::to_string(&Envelope::new(serde_json::json!({ "path": path }))) {
                Ok(json) => json_response("200 OK", &json, &[]),
                Err(_) => json_error("500 Internal Server Error", "could not encode the folder"),
            }
        }
        Err(err) if err.kind() == std::io::ErrorKind::AlreadyExists => {
            json_error("409 Conflict", "a folder with that name already exists")
        }
        Err(err) => redact("mkdir", &anyhow::Error::new(err)),
    }
}

/// Close a repository named by the `repo` id and return the updated set.
///
/// Idempotent from the client's view: an unknown id is a 404, a known one is
/// removed and its runtime/terminals stopped by the catalog rebuild.
pub(super) fn handle_close_repo(head: &RequestHead, state: &ViewerState) -> Vec<u8> {
    let entry = match lookup_repo(head, state) {
        Ok(entry) => entry,
        Err(response) => return response,
    };
    state.catalog.remove_path(&entry.path);
    persist_workspace(state);
    let repos = state.catalog.list();
    match serde_json::to_string(&Envelope::new(serde_json::json!({ "repos": repos }))) {
        Ok(json) => json_response("200 OK", &json, &[]),
        Err(_) => json_error("500 Internal Server Error", "could not encode repositories"),
    }
}

/// Mirror the served set into the shared workspace file so the next launch —
/// TUI, mirror, or viewer — starts with the same projects. No-op unless the
/// server was started with `persist` (headless `serve`); alongside the TUI,
/// the TUI owns that file. The existing per-repo view state and active tab are
/// preserved; only the open-repo list is rewritten.
fn persist_workspace(state: &ViewerState) {
    if !state.persist {
        return;
    }
    let mut ws = crate::session::load_workspace().unwrap_or_default();
    ws.repos = state.catalog.paths();
    if ws.active >= ws.repos.len() {
        ws.active = 0;
    }
    crate::session::save_workspace(&ws);
}

/// Resolve the `repo` parameter to an entry, or produce the 404 response.
pub(super) fn lookup_repo(head: &RequestHead, state: &ViewerState) -> Result<Arc<RepoEntry>, Vec<u8>> {
    let id = head
        .query_param("repo")
        .ok_or_else(|| json_error("400 Bad Request", "missing repo parameter"))?;
    state
        .catalog
        .get(&id)
        .ok_or_else(|| json_error("404 Not Found", "unknown repository"))
}

/// Map an internal error to a fixed public message, logging the detail.
///
/// git and io errors name absolute paths, symlink targets, and file sizes. The
/// client is told only that the request failed and why in general terms.
pub(super) fn redact(context: &str, err: &anyhow::Error) -> Vec<u8> {
    tracing::debug!(%err, context, "viewer: request failed");
    json_error("400 Bad Request", "request could not be served")
}