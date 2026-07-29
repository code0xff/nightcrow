//! What the served set of repositories can be asked to do, independent of how
//! the asking arrived.
//!
//! Opening, closing, and reordering are session operations, not HTTP ones. The
//! browser reaches them over HTTP and an attaching client reaches them over the
//! daemon socket, and both must land on exactly the same state change — so the
//! change lives here and each transport keeps only its own translation: status
//! codes and JSON envelopes on one side, frames on the other.
//!
//! Nothing here authenticates. Deciding who may ask is the transport's job,
//! and the two transports answer it differently: the browser presents a session
//! cookie, while reaching the socket already required being the user who owns
//! it. Folding that decision in here would put a single "trusted" flag in the
//! one place both paths share.

use super::server::ViewerState;
use crate::web::viewer::catalog::AddOutcome;
use crate::web::viewer::dto::RepoDto;

/// Why a repository could not be opened.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpenError {
    /// The request named no path at all.
    EmptyPath,
    /// The path is missing, or is not a directory.
    NotADirectory,
    /// The catalog is already at `MAX_PROJECTS`.
    TooMany,
}

/// Why a repository could not be closed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CloseError {
    /// No repository in the catalog has that id.
    UnknownRepo,
}

/// One repository as an attaching client sees it.
///
/// Carries the absolute path, which the browser's `RepoDto` deliberately does
/// not: an attached client reads git from that path itself, on the same
/// filesystem the daemon is on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionRepo {
    pub id: String,
    pub path: String,
}

/// The repositories currently served, in catalog order, as the browser sees
/// them.
pub fn list_repos(state: &ViewerState) -> Vec<RepoDto> {
    state.catalog.list()
}

/// The same set as an attaching client sees it.
pub fn list_session_repos(state: &ViewerState) -> Vec<SessionRepo> {
    state
        .catalog
        .id_paths()
        .into_iter()
        .map(|(id, path)| SessionRepo { id, path })
        .collect()
}

/// Open `raw_path` and add it to the served catalog.
///
/// The path arrives from outside, so it is expanded, checked, and resolved to
/// the worktree root before the catalog ever sees it — two spellings of one
/// repository must collapse to a single entry rather than open twice.
pub fn open_repo(state: &ViewerState, raw_path: &str) -> Result<RepoDto, OpenError> {
    let raw = raw_path.trim();
    if raw.is_empty() {
        return Err(OpenError::EmptyPath);
    }
    let expanded = crate::platform::paths::expand_tilde(raw);
    // is_dir() follows symlinks and is false for a missing path — either way it
    // cannot be served.
    if !expanded.is_dir() {
        return Err(OpenError::NotADirectory);
    }
    let resolved = crate::git::resolve_repo_path(&expanded)
        .to_string_lossy()
        .into_owned();

    match state
        .catalog
        .add_path(resolved, crate::workspace::MAX_PROJECTS)
    {
        AddOutcome::Added(repo) => {
            persist_workspace(state);
            Ok(repo)
        }
        AddOutcome::TooMany => Err(OpenError::TooMany),
    }
}

/// Close the repository named by `id`.
///
/// The catalog rebuild stops the closed repository's runtime and terminals.
/// The updated set is not returned: each transport reads it back in its own
/// projection, and both must do so afterwards anyway since another client can
/// change the set in between.
pub fn close_repo(state: &ViewerState, id: &str) -> Result<(), CloseError> {
    let entry = state.catalog.get(id).ok_or(CloseError::UnknownRepo)?;
    state.catalog.remove_path(&entry.path);
    persist_workspace(state);
    Ok(())
}

/// Reorder the catalog to `ids`.
///
/// Ids that no longer name a repository are dropped rather than refused: the
/// only way to send one is to have raced a close on another client, and the
/// catalog canonicalizes the requested order against what is actually live.
pub fn reorder_repos(state: &ViewerState, ids: &[String]) {
    let paths: Vec<String> = ids
        .iter()
        .filter_map(|id| state.catalog.get(id).map(|entry| entry.path.clone()))
        .collect();
    state.catalog.reorder(&paths);
    persist_workspace(state);
}

/// Mirror the served set into the shared workspace file so the next launch
/// starts with the same projects. No-op unless the server was started with
/// `persist` (headless `serve`); alongside the TUI, the TUI owns that file. The
/// existing per-repo view state and active tab are preserved; only the
/// open-repo list is rewritten.
fn persist_workspace(state: &ViewerState) {
    if !state.persist {
        return;
    }
    let mut ws = crate::workspace::persistence::load_workspace().unwrap_or_default();
    let active_path = ws.repos.get(ws.active).cloned();
    ws.repos = state.catalog.paths();
    ws.active = active_path
        .and_then(|path| ws.repos.iter().position(|repo| repo == &path))
        .unwrap_or(0);
    crate::workspace::persistence::save_workspace(&ws);
}
