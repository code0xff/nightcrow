//! The view state the workspace keeps for repositories it is not showing, and
//! the merge of that with the open tabs that is written back on exit.

use super::Workspace;
use super::persistence::{RepoSession, SessionState, WorkspaceState};

impl Workspace {
    /// Seed the remembered view state from the file read at startup.
    pub fn set_remembered(&mut self, sessions: Vec<RepoSession>) {
        self.remembered = sessions;
    }

    pub fn session_for(&self, repo: &str) -> Option<&SessionState> {
        self.remembered
            .iter()
            .find(|s| s.repo == repo)
            .map(|s| &s.state)
    }

    /// Every repository's view state — open tabs and remembered ones — capped
    /// and ordered for storage. Only this half: which repos are open and which
    /// is active belong to the daemon; what is selected and where it is scrolled
    /// is this client's alone (see `docs/architecture/session.md`).
    ///
    /// Open projects go last so the least-recently-used eviction never drops a
    /// tab that is currently on screen — `remember` inserts at the front.
    pub fn view_state(&self) -> Vec<RepoSession> {
        let mut into = WorkspaceState::default();
        for entry in self.remembered.iter().rev() {
            into.remember(&entry.repo, entry.state.clone());
        }
        for project in self.projects.iter().rev() {
            into.remember(project.repository_path(), project.session_to_save());
        }
        into.sessions
    }
}
