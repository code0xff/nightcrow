//! The set of projects open in one nightcrow process.
//!
//! `App` holds everything scoped to a single repository — the git views, the
//! snapshot worker, the cached `git2::Repository`, and the terminal panes
//! rooted at that workdir. `Workspace` is the layer above it: a list of those
//! per-repo states plus the index of the one currently on screen.
//!
//! The split follows the reset list in `App::change_repo`. Every field that
//! call clears is repo-scoped and therefore lives on `App`; anything it leaves
//! standing is process-wide and belongs here. Keeping that correspondence
//! exact is what makes "switch project" a cheap index change instead of a
//! teardown — `change_repo` stays the *replace this project's repo* path,
//! while switching tabs touches no project state at all.
//!
//! Only the active project is rendered and routed input. Background projects
//! keep polling their own snapshot worker so their tabs can report staleness,
//! but their PTYs are not resized (see the resize loop in `main`): a hidden
//! project's panes hold their last size the same way a hidden pane does.

use crate::app::App;

pub struct Workspace {
    /// Open projects in tab order. Never empty: the last project cannot be
    /// closed, so `active` is always a valid index.
    projects: Vec<App>,
    /// Index into `projects` of the project being rendered.
    active: usize,
}

impl Workspace {
    /// Open a workspace holding a single project.
    pub fn new(project: App) -> Self {
        Self {
            projects: vec![project],
            active: 0,
        }
    }

    pub fn active(&self) -> &App {
        // Indexing is infallible: `projects` is non-empty by construction and
        // every mutation keeps `active` in range.
        &self.projects[self.active]
    }

    pub fn active_mut(&mut self) -> &mut App {
        &mut self.projects[self.active]
    }

    /// All open projects in tab order, for rendering the tab row and for
    /// end-of-run session saves.
    pub fn projects(&self) -> &[App] {
        &self.projects
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::tests::app_with_files;

    #[test]
    fn 새_workspace는_프로젝트_하나를_활성으로_갖는다() {
        let ws = Workspace::new(app_with_files(vec!["a.rs"]));

        assert_eq!(ws.projects().len(), 1);
        assert_eq!(ws.active().repo_path, ".");
    }
}
