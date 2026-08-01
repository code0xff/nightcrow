use super::ViewerState;
use super::http_util::{json_error, json_response};
use crate::web::common::http::RequestHead;
use crate::web::viewer::catalog::RepoEntry;
use crate::web::viewer::dto::Envelope;
use crate::web::viewer::prefs::{MaximizedPanel, MaximizedUpdate, PrefsUpdate};
use crate::web::viewer::session;
use anyhow::Result;
use std::sync::Arc;

#[derive(serde::Deserialize)]
struct OpenRequest {
    path: String,
}

#[derive(serde::Deserialize)]
struct ReorderRequest {
    order: Vec<String>,
}

#[derive(serde::Deserialize)]
struct PrefsRequest {
    /// Each preference is optional so one write touches one setting and leaves
    /// the rest as they are.
    accent: Option<usize>,
    sidebar_width: Option<u32>,
    upper_pct: Option<u32>,
    /// Repo **id**, as every other client-supplied repository reference is.
    /// The server translates it to the path `prefs.rs` stores.
    active_repo: Option<String>,
    /// How one project's screen is arranged. Names its own repository rather
    /// than riding on `active_repo`: maximizing is about the project the client
    /// is looking at.
    maximized: Option<MaximizedRequest>,
}

#[derive(serde::Deserialize)]
struct MaximizedRequest {
    /// Repo id, translated to a path before it is stored.
    repo: String,
    /// `"files"`, `"terminal"`, or absent/null for nothing maximized.
    panel: Option<String>,
}

/// Store one or more viewer preferences and echo back the full stored set.
///
/// A value with a range is wrapped or clamped into it rather than rejected.
/// `active_repo` is the exception: it names a repository rather than sitting in
/// a range, and there is no nearest valid project to fold an unknown id onto.
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
    if request.accent.is_none()
        && request.sidebar_width.is_none()
        && request.upper_pct.is_none()
        && request.active_repo.is_none()
        && request.maximized.is_none()
    {
        return json_error("400 Bad Request", "no known preference in the body");
    }
    // Resolved before the write, because what is stored is the path behind the
    // id. An id no longer in the catalog is rejected rather than dropped.
    let active_path = match request.active_repo {
        Some(id) => match state.catalog.get(&id) {
            Some(entry) => Some(entry.path.clone()),
            None => return json_error("400 Bad Request", "unknown repo"),
        },
        None => None,
    };
    // Resolved before the write for the same reason, and refused the same way.
    let maximized = match request.maximized {
        Some(change) => {
            let Some(entry) = state.catalog.get(&change.repo) else {
                return json_error("400 Bad Request", "unknown repo");
            };
            let panel = match change.panel.as_deref() {
                None => None,
                Some(name) => match MaximizedPanel::parse(name) {
                    Some(panel) => Some(panel),
                    None => return json_error("400 Bad Request", "unknown panel"),
                },
            };
            Some(MaximizedUpdate {
                repo: entry.path.clone(),
                panel,
            })
        }
        None => None,
    };
    // One locked write for whatever the body carried.
    let stored = state.prefs.update(PrefsUpdate {
        accent: request.accent,
        sidebar_width: request.sidebar_width,
        upper_pct: request.upper_pct,
        active_repo: active_path,
        maximized,
    });
    // One snapshot for both, as the bootstrap does: the two are read together
    // by the client, and an id in one that is missing from the other describes
    // a served set that never existed.
    let served = state
        .catalog
        .list_with_active(stored.active_repo.as_deref(), &stored.maximized);
    match serde_json::to_string(&Envelope::new(serde_json::json!({
        "accent": stored.accent,
        "sidebar_width": stored.sidebar_width,
        "upper_pct": stored.upper_pct,
        "active_repo": served.active,
        "maximized": served.maximized,
    }))) {
        Ok(json) => json_response("200 OK", &json, &[]),
        Err(_) => json_error("500 Internal Server Error", "could not encode preferences"),
    }
}

/// Open a repository from the browser and add it to the served catalog.
///
/// The path is user-supplied but the response is public, so a bad path yields a
/// generic message.
pub(super) fn handle_open_repo(body: &str, state: &ViewerState) -> Vec<u8> {
    let request: OpenRequest = match serde_json::from_str(body) {
        Ok(request) => request,
        Err(_) => return json_error("400 Bad Request", "expected a JSON body with a path"),
    };
    match session::open_repo(state, &request.path) {
        Ok(repo) => {
            match serde_json::to_string(&Envelope::new(serde_json::json!({ "repo": repo }))) {
                Ok(json) => json_response("200 OK", &json, &[]),
                Err(_) => json_error("500 Internal Server Error", "could not encode repository"),
            }
        }
        Err(session::OpenError::EmptyPath) => json_error("400 Bad Request", "a path is required"),
        Err(session::OpenError::NotADirectory) => {
            json_error("400 Bad Request", "no such directory")
        }
        Err(session::OpenError::TooMany) => json_error(
            "409 Conflict",
            "the maximum number of repositories is already open",
        ),
    }
}

#[derive(serde::Deserialize)]
struct MkdirRequest {
    /// The directory to create the new folder inside.
    path: String,
    /// The new folder's name. Must be a single plain path segment.
    name: String,
}

/// Create a new folder inside a directory the picker is browsing.
///
/// The parent is confined only as much as `browse` is, but `name` is held to a
/// single plain segment: separators, `..`, a leading `.` (which also rules out
/// `.git`), and NUL are all rejected. Combined with canonicalizing the parent
/// first, the created folder can only ever land directly under the browsed
/// directory.
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
    let parent = crate::platform::paths::expand_tilde(request.path.trim());
    // is_dir() follows symlinks and is false for a missing path.
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
            let path = crate::platform::paths::for_display(&target).into_owned();
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
pub(super) fn handle_close_repo(head: &RequestHead, state: &ViewerState) -> Vec<u8> {
    let Some(id) = head.query_param("repo") else {
        return json_error("400 Bad Request", "missing repo parameter");
    };
    match session::close_repo(state, &id) {
        Ok(()) => {
            let repos = session::list_repos(state);
            match serde_json::to_string(&Envelope::new(serde_json::json!({ "repos": repos }))) {
                Ok(json) => json_response("200 OK", &json, &[]),
                Err(_) => json_error("500 Internal Server Error", "could not encode repositories"),
            }
        }
        Err(session::CloseError::UnknownRepo) => json_error("404 Not Found", "unknown repository"),
    }
}

pub(super) fn handle_reorder_repos(body: &str, state: &ViewerState) -> Vec<u8> {
    let request: ReorderRequest = match serde_json::from_str(body) {
        Ok(request) => request,
        Err(_) => return json_error("400 Bad Request", "expected a JSON body with an order"),
    };
    session::reorder_repos(state, &request.order);
    let repos = session::list_repos(state);
    match serde_json::to_string(&Envelope::new(serde_json::json!({ "repos": repos }))) {
        Ok(json) => json_response("200 OK", &json, &[]),
        Err(_) => json_error("500 Internal Server Error", "could not encode repositories"),
    }
}

/// Re-read `config.toml` and report what was applied.
///
/// The body is ignored and no configuration is accepted from the request: the
/// file on the server's disk is what is read. Deciding who may ask happened
/// before this — the route is behind the same session cookie as every other
/// mutation.
///
/// A refusal is the operator's own message, not a redacted one. These name the
/// offending key in their own config file; they carry no repository contents or
/// credentials.
pub(super) fn handle_reload_config(state: &ViewerState) -> Vec<u8> {
    match crate::web::viewer::reload::reload_config(state) {
        Ok(report) => {
            let body = Envelope::new(serde_json::json!({ "summary": report.summary() }));
            match serde_json::to_string(&body) {
                Ok(json) => json_response("200 OK", &json, &[]),
                Err(_) => json_error("500 Internal Server Error", "could not encode the report"),
            }
        }
        // 422 rather than 400: the request was well-formed, and what could not be
        // processed is the file it points the server at.
        Err(err) => json_error("422 Unprocessable Entity", &err.to_string()),
    }
}

/// Resolve the `repo` parameter to an entry, or produce the 404 response.
pub(super) fn lookup_repo(
    head: &RequestHead,
    state: &ViewerState,
) -> Result<Arc<RepoEntry>, Vec<u8>> {
    let id = head
        .query_param("repo")
        .ok_or_else(|| json_error("400 Bad Request", "missing repo parameter"))?;
    state
        .catalog
        .get(&id)
        .ok_or_else(|| json_error("404 Not Found", "unknown repository"))
}

/// Map an internal error to a fixed public message, logging the detail.
/// git and io errors name absolute paths, symlink targets, and file sizes.
pub(super) fn redact(context: &str, err: &anyhow::Error) -> Vec<u8> {
    tracing::debug!(%err, context, "viewer: request failed");
    json_error("400 Bad Request", "request could not be served")
}
