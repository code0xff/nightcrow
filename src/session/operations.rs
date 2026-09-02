//! Session operations independent of how the request arrived: the browser and
//! an attaching client must land on exactly the same state change, so the
//! change lives here and each transport keeps only its own translation.
//! Nothing here authenticates — deciding who may ask is the transport's job.

use super::SessionState;
use crate::session::catalog::{AddOutcome, RepoInfo};

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

/// One repository as an attaching client sees it, with the absolute path the
/// browser's `RepoDto` deliberately omits: an attached client reads git from
/// that path itself.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionRepo {
    pub id: String,
    pub path: String,
}

/// The repositories currently served, in catalog order, as the browser sees
/// them.
pub fn list_repos(state: &SessionState) -> Vec<RepoInfo> {
    state.catalog.list()
}

/// The same set as an attaching client sees it.
pub fn list_session_repos(state: &SessionState) -> Vec<SessionRepo> {
    state
        .catalog
        .id_paths()
        .into_iter()
        .map(|(id, path)| SessionRepo { id, path })
        .collect()
}

/// Open `raw_path` and add it to the served catalog.
///
/// Resolved to the worktree root before the catalog ever sees it: two
/// spellings of one repository must collapse to a single entry.
pub fn open_repo(state: &SessionState, raw_path: &str) -> Result<RepoInfo, OpenError> {
    let raw = raw_path.trim();
    if raw.is_empty() {
        return Err(OpenError::EmptyPath);
    }
    let expanded = crate::platform::paths::expand_tilde(raw);
    // is_dir() follows symlinks and is false for a missing path — either way
    // unservable.
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
            // Opening is also a statement about where the client wants to be;
            // leaving the focus behind would background the tab someone just
            // asked for, on their own screen and everyone else's.
            if let Some(entry) = state.catalog.get(&repo.id) {
                state.prefs.set_active_repo(entry.path.clone());
            }
            persist_workspace(state);
            Ok(repo)
        }
        AddOutcome::TooMany => Err(OpenError::TooMany),
    }
}

/// The repository the session is focused on, as the id clients speak.
///
/// Falls back to the first served repository when nothing has been focused yet,
/// or when what is on file is no longer served. `None` only when nothing is open
/// at all. The stored value is a path, because ids only live as long as the
/// process (see [`super::prefs`]), so this is where it is translated back.
pub fn active_repo(state: &SessionState) -> Option<String> {
    active_repo_from(state, state.prefs.get().active_repo.as_deref())
}

/// [`active_repo`] against a `stored` value the caller already has.
///
/// For a caller that must not read the preference twice: what it decides and
/// what it later writes against have to come from one read, or a focus landing
/// between the two is mistaken for the value the decision was made against.
fn active_repo_from(state: &SessionState, stored: Option<&str>) -> Option<String> {
    stored
        .and_then(|path| state.catalog.id_of_path(path))
        .or_else(|| {
            state
                .catalog
                .id_paths()
                .into_iter()
                .next()
                .map(|(id, _)| id)
        })
}

/// Focus the repository named by `id` for the whole session.
///
/// Shared, not per-client: the daemon owns which project is in front, and
/// every client renders the one the session names.
pub fn focus_repo(state: &SessionState, id: &str) -> Result<(), CloseError> {
    let entry = state.catalog.get(id).ok_or(CloseError::UnknownRepo)?;
    state.prefs.set_active_repo(entry.path.clone());
    Ok(())
}

/// The accent every surface of this session paints in.
pub fn accent(state: &SessionState) -> usize {
    state.prefs.get().accent
}

/// Set the session's accent, returning what was stored.
///
/// Shared like the active project. An index past the end of the cycle wraps
/// rather than being refused, matching `Accent::from_index`.
pub fn set_accent(state: &SessionState, accent: usize) -> usize {
    state.prefs.set_accent(accent).accent
}

/// Close the repository named by `id`.
///
/// The catalog rebuild stops the closed repository's runtime and terminals.
/// The updated set is not returned: each transport reads it back in its own
/// projection afterwards anyway, since another client can change the set.
pub fn close_repo(state: &SessionState, id: &str) -> Result<(), CloseError> {
    let entry = state.catalog.get(id).ok_or(CloseError::UnknownRepo)?;
    // One read of the preference; both the decision and the write condition
    // come from it. Reading twice leaves a gap a focus landing in between
    // would be mistaken into.
    //
    // Not necessarily the closing path, which is why the condition is this
    // rather than a comparison against it: the preference may be empty, or
    // name a project this session does not serve. Comparing against the
    // closing path skipped those silently — the fallback answered correctly
    // only while the successor stayed first.
    let focus_before = state.prefs.get().active_repo;
    // Read before the close: it is a position in the set about to change, and
    // only interesting when the closing tab is the one in front.
    let successor = (active_repo_from(state, focus_before.as_deref()).as_deref() == Some(id))
        .then(|| successor_of(state, id))
        .flatten();
    state.catalog.remove_path(&entry.path);
    // Said outright rather than left to `active_repo`'s fallback, which sends
    // a close to the first tab from wherever they were; the TUI has picked the
    // neighbour since it had tabs (`workspace::close_at`).
    if let Some(path) = successor
        // Still served — another client can have closed it in between, and
        // recording a path nothing resolves would land everyone on the first
        // tab fallback this exists to stop.
        && state.catalog.id_of_path(&path).is_some()
    {
        // Only while the preference is still what it was when this decided —
        // atomic against another focus inside the preference store's locked
        // write, but it cannot see the catalog, whose lock this must not hold
        // at the same time. Guarantees *the close* does not overwrite a focus
        // made meanwhile, not that such a focus survives: a browser records
        // where it landed and last assertion wins, as for every other switch.
        state
            .prefs
            .set_active_repo_if(focus_before.as_deref(), path);
    }
    persist_workspace(state);
    Ok(())
}

/// The tab to put in front once `id` closes: the one after it, or the one
/// before when it is last. Its *path*, because the id is about to stop naming
/// anything. The same rule `workspace::close_at` applies on the TUI, so
/// adopting it moves nothing.
///
/// `None` when the set holds nothing else; nothing is written then, and the
/// stale entry costs nothing because it no longer resolves.
fn successor_of(state: &SessionState, id: &str) -> Option<String> {
    let served = state.catalog.id_paths();
    let closing = served.iter().position(|(served_id, _)| served_id == id)?;
    served
        .get(closing + 1)
        .or_else(|| closing.checked_sub(1).and_then(|before| served.get(before)))
        .map(|(_, path)| path.clone())
}

/// Reorder the catalog to `ids`.
///
/// Ids that no longer name a repository are dropped rather than refused: the
/// only way to send one is to have raced a close on another client.
///
/// The tab in front stays with its repository. That needs saying because
/// [`active_repo`]'s fallback names the *first served* repository while nothing
/// has been focused, and reordering changes which one that is — so the fallback
/// is pinned to what it currently names before the slots move under it.
pub fn reorder_repos(state: &SessionState, ids: &[String]) {
    let focus_before = state.prefs.get().active_repo;
    let front = focus_before
        .as_deref()
        .and_then(|path| state.catalog.id_of_path(path))
        .is_none()
        .then(|| active_repo_from(state, focus_before.as_deref()))
        .flatten()
        .and_then(|id| state.catalog.get(&id).map(|entry| entry.path.clone()));
    let paths: Vec<String> = ids
        .iter()
        .filter_map(|id| state.catalog.get(id).map(|entry| entry.path.clone()))
        .collect();
    // Before the slots move rather than after: the watcher reads the session on
    // its own tick, and one landing between the two statements would see the new
    // order while the fallback still named the old first tab — broadcasting the
    // front-tab jump this pinning exists to prevent.
    if let Some(path) = front {
        // Only while the preference is still what the fallback was read
        // against, for the reason the close does the same: a focus arriving in
        // between must win rather than be overwritten by a reorder.
        state
            .prefs
            .set_active_repo_if(focus_before.as_deref(), path);
    }
    state.catalog.reorder(&paths);
    persist_workspace(state);
}

/// Mirror the served set into the shared workspace file so the next launch
/// starts with the same projects. No-op unless the server was started with
/// `persist` (a headless daemon); alongside the TUI, the TUI owns that file.
/// Only the open-repo list is rewritten.
fn persist_workspace(state: &SessionState) {
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
