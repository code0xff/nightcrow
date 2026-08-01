//! What the served set of repositories can be asked to do, independent of how
//! the asking arrived.
//!
//! Opening, closing, and reordering are session operations, not HTTP ones. The
//! browser reaches them over HTTP and an attaching client reaches them over the
//! daemon socket, and both must land on exactly the same state change — so the
//! change lives here and each transport keeps only its own translation.
//!
//! Nothing here authenticates. Deciding who may ask is the transport's job.

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

/// One repository as an attaching client sees it.
///
/// Carries the absolute path, which the browser's `RepoDto` deliberately does
/// not: an attached client reads git from that path itself.
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
/// The path arrives from outside, so it is expanded, checked, and resolved to
/// the worktree root before the catalog ever sees it — two spellings of one
/// repository must collapse to a single entry.
pub fn open_repo(state: &SessionState, raw_path: &str) -> Result<RepoInfo, OpenError> {
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
            // Opening is also a statement about where the client wants to be, so
            // it focuses. Every client follows the session's active project, and
            // leaving the focus behind would put the tab someone just asked for
            // in the background — on their own screen and everyone else's.
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
    let stored = state
        .prefs
        .get()
        .active_repo
        .as_deref()
        .and_then(|path| state.catalog.id_of_path(path));
    stored.or_else(|| {
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
/// Which project is in front is shared, not per-client: the daemon owns it, and
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
/// Shared like the active project rather than kept per surface. An index past
/// the end of the cycle wraps rather than being refused, matching
/// `Accent::from_index`.
pub fn set_accent(state: &SessionState, accent: usize) -> usize {
    state.prefs.set_accent(accent).accent
}

/// Close the repository named by `id`.
///
/// The catalog rebuild stops the closed repository's runtime and terminals.
/// The updated set is not returned: each transport reads it back in its own
/// projection, and both must do so afterwards anyway since another client can
/// change the set in between.
pub fn close_repo(state: &SessionState, id: &str) -> Result<(), CloseError> {
    let entry = state.catalog.get(id).ok_or(CloseError::UnknownRepo)?;
    // Read before the close, because it is a position in the set that is about
    // to change, and only interesting when the tab being closed is the one in
    // front — closing a background project must leave the focus where it is.
    let successor = (active_repo(state).as_deref() == Some(id))
        .then(|| successor_of(state, id))
        .flatten();
    state.catalog.remove_path(&entry.path);
    // Said outright rather than left to `active_repo`'s fallback. That fallback
    // answers "nothing has been focused yet, or what is on file is no longer
    // served" with the first repository, which is right for a fresh session and
    // wrong for a close: it sent everyone to the first tab from wherever they
    // were. The TUI has picked the neighbour since it had tabs
    // (`workspace::close_at`) and was overruled by this a beat later.
    if let Some(path) = successor
        // Still served. The successor was read from the set before the close,
        // and another client can have closed it in between — recording a path
        // nothing resolves would leave every surface on `active_repo`'s
        // fallback, which is the first tab this exists to stop landing on.
        && state.catalog.id_of_path(&path).is_some()
    {
        // Only while the focus is still the project this decided about.
        // Compared inside the preference store's own locked write, so against
        // another focus it is atomic; what it cannot see is the catalog, which
        // has a lock of its own that this must not hold at the same time.
        //
        // What this guarantees is that *the close* does not overwrite a focus
        // made meanwhile — not that such a focus survives. A browser that
        // closed the tab then records where it landed, from the one place its
        // selection settles (`useRepoPoll`), and that write is a client saying
        // where it is rather than a close deciding for everyone. Last assertion
        // wins there, as it does for every other switch. A TUI close asserts
        // nothing after the fact, so for it this holds outright.
        state
            .prefs
            .set_active_repo_if(Some(entry.path.as_str()), path);
    }
    persist_workspace(state);
    Ok(())
}

/// The tab to put in front once `id` closes: the one after it, or the one
/// before when it is last. Its *path*, because that is what the preference
/// stores and the id is about to stop naming anything.
///
/// The same rule browsers use, and the same one `workspace::close_at` already
/// applies on the TUI — so the answer this records is the one that client had
/// picked for itself, and adopting it moves nothing.
///
/// `None` when the set holds nothing else, which is the empty screen. Nothing
/// is written then: there is no tab to name, and the stale entry costs nothing
/// because it no longer resolves.
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
/// only way to send one is to have raced a close on another client, and the
/// catalog canonicalizes the requested order against what is actually live.
pub fn reorder_repos(state: &SessionState, ids: &[String]) {
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
