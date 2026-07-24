//! The set of projects open in one nightcrow process.
//!
//! `App` holds everything scoped to a single repository — the git views, the
//! snapshot worker, the cached `git2::Repository`, and the terminal panes
//! rooted at that workdir. `Workspace` is the layer above it: a list of those
//! per-repo states plus the index of the one currently on screen.
//!
//! Anything scoped to a repository lives on `App`; anything process-wide
//! belongs here. Switching tabs is therefore a cheap index change that touches
//! no project state at all, and there is no operation that repoints a project
//! at another repo — closing the tab drops the `App`, and its own types tear
//! the worker and the panes down. That is why nothing here needs a
//! field-by-field reset list to keep in sync.
//!
//! The list may be empty. A bare launch starts that way and closing the last
//! tab returns to it, so `active()` yields an `Option` and the open-repo
//! dialog lives here rather than on a project — with none open, raising it is
//! the only thing left to do.
//!
//! Only the active project is rendered, routed input, and resized, but every
//! project *drains* its queues each tick (see the loop in `main`): the snapshot
//! worker and PTY reader keep producing into unbounded channels whether or not
//! a tab is on screen. Applying a snapshot is active-only, since that runs a
//! full diff. A hidden project's panes hold their last size the same way a
//! hidden pane does.

mod repo_input;

pub use repo_input::RepoInputResult;

use crate::app::{App, Notice, NoticeKind};
use crate::session::{MAX_REMEMBERED, RepoSession, SessionState, WorkspaceState};
use crate::ui::status_view::RepoInput;
use crossterm::event::KeyEvent;

/// Upper bound on open projects, matching the F1..F10 switch keys. Panes cap
/// at 8 for the same reason: a tab you cannot reach by key is a tab that is
/// hard to find, so the key space sets the limit rather than memory.
pub const MAX_PROJECTS: usize = 10;

pub struct Workspace {
    /// Open projects in tab order. May be empty — nightcrow starts that way
    /// and every tab can be closed, so `active` is only meaningful when this
    /// is non-empty.
    projects: Vec<App>,
    /// Index into `projects` of the project being rendered.
    active: usize,
    /// The open-repo dialog.
    ///
    /// Process-level rather than per-project because it has to work with no
    /// project open — which is exactly when it matters most, since opening one
    /// is the only thing to do from there.
    pub repo_input: RepoInput,
    /// Notice shown while no project is open. A project owns its own notice
    /// (its row also carries its repo identity), so this slot is only read on
    /// the empty screen.
    empty_notice: Option<Notice>,
    /// The configured leader chord, kept here so the empty screen can label
    /// and recognise it with no project to ask.
    leader: KeyEvent,
    /// Prefix armed on the empty screen. A project has its own flag; the two
    /// never both apply, since a key is dispatched to exactly one of them.
    empty_prefix_armed: bool,
    /// View state for repositories that are not open right now — read at
    /// startup, added to as tabs close. Open projects are not in here; their
    /// state is read off the `App` when the file is written.
    remembered: Vec<RepoSession>,
}

impl Workspace {
    /// Open a workspace with no projects.
    pub fn new(leader: KeyEvent) -> Self {
        Self {
            projects: Vec::new(),
            active: 0,
            repo_input: RepoInput::default(),
            empty_notice: None,
            leader,
            empty_prefix_armed: false,
            remembered: Vec::new(),
        }
    }

    /// Seed the remembered view state, once, from the file read at startup.
    pub fn set_remembered(&mut self, sessions: Vec<RepoSession>) {
        self.remembered = sessions;
    }

    /// The saved view state for `repo`, whether it comes from a closed tab
    /// this run or from the file read at startup.
    pub fn session_for(&self, repo: &str) -> Option<&SessionState> {
        self.remembered
            .iter()
            .find(|s| s.repo == repo)
            .map(|s| &s.state)
    }

    /// Everything to write out: the open tabs, which was in front, and every
    /// repository's view state — the open ones read live, the rest as
    /// remembered. Open projects go first so the least-recently-used eviction
    /// never drops a tab that is currently on screen.
    pub fn to_persisted(&self) -> WorkspaceState {
        let mut persisted = WorkspaceState {
            repos: self.projects.iter().map(|p| p.repo_path.clone()).collect(),
            active: self.active,
            sessions: Vec::new(),
        };
        // `remember` inserts at the front, so applying the remembered entries
        // first and the open ones last leaves the open ones foremost.
        for entry in self.remembered.iter().rev() {
            persisted.remember(&entry.repo, entry.state.clone());
        }
        for project in self.projects.iter().rev() {
            persisted.remember(&project.repo_path, project.session_to_save());
        }
        persisted
    }

    /// Matches only the bare leader chord; any extra modifier (Alt/Shift/Super/
    /// Hyper/Meta) is a different chord and passes through. See `App::is_leader_key`.
    pub fn is_leader_key(&self, key: KeyEvent) -> bool {
        key.code == self.leader.code && key.modifiers == self.leader.modifiers
    }

    pub fn leader(&self) -> KeyEvent {
        self.leader
    }

    pub fn prefix_armed(&self) -> bool {
        self.empty_prefix_armed
    }

    pub fn arm_prefix(&mut self) {
        self.empty_prefix_armed = true;
    }

    pub fn cancel_prefix(&mut self) {
        self.empty_prefix_armed = false;
    }

    /// The project on screen, or `None` when no project is open.
    pub fn active(&self) -> Option<&App> {
        self.projects.get(self.active)
    }

    pub fn active_mut(&mut self) -> Option<&mut App> {
        self.projects.get_mut(self.active)
    }

    /// The active project and the dialog together, borrowed from disjoint
    /// fields in one call so a frame can render both without the borrow
    /// checker seeing an overlap.
    pub fn render_parts(&mut self) -> (Option<&mut App>, &RepoInput) {
        (self.projects.get_mut(self.active), &self.repo_input)
    }

    /// Raise a notice on the active project, or on the empty screen when
    /// there is none. Callers do not have to know which case they are in.
    pub fn raise_notice(&mut self, kind: NoticeKind, text: impl Into<String>) {
        match self.projects.get_mut(self.active) {
            Some(project) => project.raise_notice(kind, text),
            None => self.empty_notice = Some(Notice::new(kind, text)),
        }
    }

    /// Drop the current notice if it was raised by `kind`.
    pub fn clear_notice(&mut self, kind: NoticeKind) {
        match self.projects.get_mut(self.active) {
            Some(project) => project.clear_notice(kind),
            None => {
                if self.empty_notice.as_ref().is_some_and(|n| n.kind == kind) {
                    self.empty_notice = None;
                }
            }
        }
    }

    /// The notice to render when no project is open.
    pub fn empty_notice(&self) -> Option<&Notice> {
        self.empty_notice.as_ref()
    }

    /// All open projects in tab order, for rendering the tab row and for
    /// end-of-run session saves.
    pub fn projects(&self) -> &[App] {
        &self.projects
    }

    pub fn active_index(&self) -> usize {
        self.active
    }

    /// All open projects, mutably — for the per-tick polling that every
    /// project needs whether or not it is on screen.
    pub fn projects_mut(&mut self) -> &mut [App] {
        &mut self.projects
    }

    /// Whether another project would exceed `MAX_PROJECTS`. Checked before
    /// *building* a project, since construction spawns PTYs and runs the
    /// configured startup commands — work that must not happen for a tab that
    /// `add` is about to refuse.
    pub fn is_full(&self) -> bool {
        self.projects.len() >= MAX_PROJECTS
    }

    /// Open `project` in a new tab and make it active.
    ///
    /// Returns `false` when already at `MAX_PROJECTS`, leaving the workspace
    /// untouched; the caller reports that to the user rather than silently
    /// dropping the request.
    pub fn add(&mut self, project: App) -> bool {
        if self.projects.len() >= MAX_PROJECTS {
            return false;
        }
        // Whatever the empty screen was reporting is answered by this: it is
        // no longer the empty screen. Left standing, a stale message would
        // reappear the moment the last tab was closed again.
        self.empty_notice = None;
        // Same reasoning as `switch`: the outgoing project's press can no
        // longer be paired, since the release will be routed to the new one.
        if let Some(previous) = self.projects.get_mut(self.active) {
            previous.release_pending_press_in_place();
        }
        self.projects.push(project);
        self.active = self.projects.len() - 1;
        true
    }

    /// Close the active project and focus its neighbour, leaving the
    /// workspace empty when it was the last one.
    ///
    /// Returns `false` only when there was nothing to close.
    ///
    /// Dropping the removed `App` is what tears the project down — its
    /// `SnapshotChannel` joins the worker thread and its `TerminalState` kills
    /// the panes' child processes. There is no field-by-field reset to keep in
    /// sync, which is why closing a tab is the way to leave a repo.
    pub fn close_active(&mut self) -> bool {
        if self.projects.is_empty() {
            return false;
        }
        // Carry the closing project's view state over, or reopening the repo
        // would restore whatever it looked like at the last shutdown instead.
        let closing = self.projects.remove(self.active);
        self.remembered.retain(|s| s.repo != closing.repo_path);
        self.remembered.insert(
            0,
            RepoSession {
                repo: closing.repo_path.clone(),
                state: closing.session_to_save(),
            },
        );
        // Capped here as well as on the way to disk: a long-lived process that
        // opens and closes many repositories would otherwise hold every
        // session it had ever seen, tree expansion paths and all, and rescan
        // that history on every save.
        self.remembered.truncate(MAX_REMEMBERED);
        // Focus the tab that slid into this slot; closing the rightmost tab
        // falls back to its left neighbour. Saturates to 0 when now empty.
        self.active = self.active.min(self.projects.len().saturating_sub(1));
        true
    }

    /// Focus the project at `index`. Out-of-range indices are ignored so a
    /// key or click naming an absent tab is inert rather than a panic.
    pub fn switch(&mut self, index: usize) {
        if index >= self.projects.len() || index == self.active {
            return;
        }
        // Safe to index: the bound check above proved `active` is in range
        // whenever `projects` is non-empty, and an empty list fails it.
        // A press still awaiting its release can no longer be paired: the
        // release will be routed to the newly active project. Deliver it to
        // the pane that saw the press instead of dropping the record — that
        // program's PTY is still alive, and with no release it would sit in a
        // drag or selection state, while a leftover record could pair with an
        // unrelated release later.
        self.projects[self.active].release_pending_press_in_place();
        self.active = index;
    }

    /// Index of the project already open on `repo_path`, if any. Lets the
    /// caller focus an open project instead of opening a second tab onto the
    /// same repo — two tabs sharing a workdir would show identical git state
    /// while racing each other's snapshot workers.
    pub fn index_of_repo(&self, repo_path: &str) -> Option<usize> {
        self.projects.iter().position(|p| p.repo_path == repo_path)
    }
}

#[cfg(test)]
mod tests;
