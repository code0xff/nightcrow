//! Per-repo state (`App`) held in a list, with the active tab index on top.
//! Closing a tab drops the `App`, which tears down its worker and panes — no
//! field-by-field reset to keep in sync. The list may be empty, so `active()`
//! yields an `Option` and the open-repo dialog lives here rather than on a
//! project. Every project drains its queues each tick whether or not it is on
//! screen; snapshots apply to the active one only.

mod path_complete;
mod repo_input;

pub use repo_input::RepoInputResult;

pub(crate) mod persistence;

use self::persistence::{MAX_REMEMBERED, RepoSession, SessionState, WorkspaceState};
use crate::app::{App, Notice, NoticeKind};
use crate::ui::status_view::RepoInput;
use crossterm::event::KeyEvent;

/// Upper bound on open projects, matching the F1..F10 switch keys. A tab you
/// cannot reach by key is hard to find, so the key space sets the limit.
pub const MAX_PROJECTS: usize = 10;

pub struct Workspace {
    projects: Vec<App>,
    active: usize,
    /// Lives at process level because it must work with no project open.
    pub repo_input: RepoInput,
    /// Notice shown only on the empty screen; a project owns its own.
    empty_notice: Option<Notice>,
    leader: KeyEvent,
    /// Prefix armed on the empty screen; a project has its own flag, and a key
    /// is dispatched to exactly one of them.
    empty_prefix_armed: bool,
    /// View state for repos not currently open. Open projects are read live
    /// off the `App` at save time rather than stored here.
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

    pub fn session_for(&self, repo: &str) -> Option<&SessionState> {
        self.remembered
            .iter()
            .find(|s| s.repo == repo)
            .map(|s| &s.state)
    }

    /// Open projects go first so the least-recently-used eviction never drops
    /// a tab that is currently on screen.
    pub fn to_persisted(&self) -> WorkspaceState {
        let mut persisted = WorkspaceState {
            repos: self.projects.iter().map(|p| p.repo_path.clone()).collect(),
            active: self.active,
            sessions: Vec::new(),
        };
        // `remember` inserts at the front, so remembered entries go first and
        // open ones last to leave the open ones foremost.
        for entry in self.remembered.iter().rev() {
            persisted.remember(&entry.repo, entry.state.clone());
        }
        for project in self.projects.iter().rev() {
            persisted.remember(&project.repo_path, project.session_to_save());
        }
        persisted
    }

    /// Matches only the bare leader chord; any extra modifier passes through.
    /// See `App::is_leader_key`.
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
    /// fields so a frame can render both without a borrow-checker conflict.
    pub fn render_parts(&mut self) -> (Option<&mut App>, &RepoInput) {
        (self.projects.get_mut(self.active), &self.repo_input)
    }

    /// Raise a notice on the active project or the empty screen; callers need
    /// not know which case they are in.
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

    /// All open projects in tab order, for the tab row and end-of-run saves.
    pub fn projects(&self) -> &[App] {
        &self.projects
    }

    pub fn active_index(&self) -> usize {
        self.active
    }

    /// All open projects, mutably — for per-tick polling of every project.
    pub fn projects_mut(&mut self) -> &mut [App] {
        &mut self.projects
    }

    /// Checked before *building* a project: construction spawns PTYs and runs
    /// startup commands, which must not happen for a tab `add` will refuse.
    pub fn is_full(&self) -> bool {
        self.projects.len() >= MAX_PROJECTS
    }

    /// Open `project` in a new tab and make it active. Returns `false` at
    /// `MAX_PROJECTS`, leaving the workspace untouched.
    pub fn add(&mut self, project: App) -> bool {
        if self.projects.len() >= MAX_PROJECTS {
            return false;
        }
        // The empty screen's notice is answered by this; left standing it
        // would reappear when the last tab closes again.
        self.empty_notice = None;
        // The outgoing project's press can no longer be paired — the release
        // will route to the new project (same reasoning as `switch`).
        if let Some(previous) = self.projects.get_mut(self.active) {
            previous.release_pending_press_in_place();
        }
        self.projects.push(project);
        self.active = self.projects.len() - 1;
        true
    }

    /// Close the active project and focus its neighbour, leaving the workspace
    /// empty when it was the last. Returns `false` only when nothing was open.
    /// Dropping the `App` tears the project down (worker join, child kills);
    /// there is no field-by-field reset, which is why closing is the only way
    /// to leave a repo.
    pub fn close_active(&mut self) -> bool {
        if self.projects.is_empty() {
            return false;
        }
        // Carry the closing project's view state, or reopening would restore
        // the last-shutdown snapshot instead.
        let closing = self.projects.remove(self.active);
        self.remembered.retain(|s| s.repo != closing.repo_path);
        self.remembered.insert(
            0,
            RepoSession {
                repo: closing.repo_path.clone(),
                state: closing.session_to_save(),
            },
        );
        // Capped here as on disk: a long-lived process opening and closing
        // many repos would otherwise rescan the whole history every save.
        self.remembered.truncate(MAX_REMEMBERED);
        // Closing the rightmost tab falls back to its left neighbour;
        // saturates to 0 when now empty.
        self.active = self.active.min(self.projects.len().saturating_sub(1));
        true
    }

    /// Out-of-range indices are ignored so a key or click naming an absent
    /// tab is inert rather than a panic.
    pub fn switch(&mut self, index: usize) {
        if index >= self.projects.len() || index == self.active {
            return;
        }
        // A press still awaiting its release can no longer be paired: the
        // release will route to the newly active project. Deliver it to the
        // pane that saw the press instead of dropping the record — that PTY
        // is still alive, and with no release it would sit in a drag or
        // selection state, while a leftover record could pair with an
        // unrelated release later.
        self.projects[self.active].release_pending_press_in_place();
        self.active = index;
    }

    /// Lets the caller focus an open project instead of opening a second tab
    /// onto the same repo — two tabs sharing a workdir would show identical
    /// git state while racing each other's snapshot workers.
    pub fn index_of_repo(&self, repo_path: &str) -> Option<usize> {
        self.projects.iter().position(|p| p.repo_path == repo_path)
    }
}

#[cfg(test)]
mod tests;
