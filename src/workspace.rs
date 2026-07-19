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
//! Only the active project is rendered and routed input, but *every* project
//! is polled each tick. Its snapshot worker and PTY reader keep producing into
//! unbounded channels whether or not its tab is on screen, so a background
//! project that went undrained would grow without bound until the user
//! happened to switch back to it.
//!
//! Background projects are not resized, though (see the resize loop in
//! `main`): a hidden project's panes hold their last size the same way a
//! hidden pane does.

use crate::app::App;

/// Upper bound on open projects, matching the F1..F10 switch keys. Panes cap
/// at 8 for the same reason: a tab you cannot reach by key is a tab that is
/// hard to find, so the key space sets the limit rather than memory.
pub const MAX_PROJECTS: usize = 10;

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
        self.projects.push(project);
        self.active = self.projects.len() - 1;
        true
    }

    /// Close the active project and focus its neighbour.
    ///
    /// Returns `false` when only one project is open: a workspace with no
    /// project has nothing to render and no repo to act on, so quitting is the
    /// way out of the last tab, not closing it.
    ///
    /// Dropping the removed `App` is what tears the project down — its
    /// `SnapshotChannel` joins the worker thread and its `TerminalState` kills
    /// the panes' child processes. Nothing is cleared by hand here, unlike
    /// `change_repo`, which has to reset in place because the `App` survives.
    pub fn close_active(&mut self) -> bool {
        if self.projects.len() <= 1 {
            return false;
        }
        self.projects.remove(self.active);
        // Focus the tab that slid into this slot; closing the rightmost tab
        // falls back to its left neighbour.
        self.active = self.active.min(self.projects.len() - 1);
        true
    }

    /// Focus the project at `index`. Out-of-range indices are ignored so a
    /// key or click naming an absent tab is inert rather than a panic.
    pub fn switch(&mut self, index: usize) {
        if index >= self.projects.len() || index == self.active {
            return;
        }
        // A press still awaiting its release can no longer be paired: the
        // release will be routed to the newly active project. Drop it so a
        // later unrelated release cannot pair with this stale press.
        self.projects[self.active].pending_mouse_press = None;
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
mod tests {
    use super::*;
    use crate::app::tests::app_with_files;

    /// A project distinguishable from its siblings by `repo_path`, which is
    /// what the tab row labels and `index_of_repo` match on.
    fn project_at(path: &str) -> App {
        let mut app = app_with_files(vec!["a.rs"]);
        app.repo_path = path.to_string();
        app
    }

    fn paths(ws: &Workspace) -> Vec<&str> {
        ws.projects().iter().map(|p| p.repo_path.as_str()).collect()
    }

    #[test]
    fn 새_workspace는_프로젝트_하나를_활성으로_갖는다() {
        let ws = Workspace::new(app_with_files(vec!["a.rs"]));

        assert_eq!(ws.projects().len(), 1);
        assert_eq!(ws.active().repo_path, ".");
    }

    #[test]
    fn 프로젝트를_추가하면_끝에_붙고_활성이_된다() {
        let mut ws = Workspace::new(project_at("/a"));

        assert!(ws.add(project_at("/b")));

        assert_eq!(paths(&ws), vec!["/a", "/b"]);
        assert_eq!(ws.active().repo_path, "/b");
    }

    #[test]
    fn 상한에_도달하면_추가를_거부하고_활성을_유지한다() {
        let mut ws = Workspace::new(project_at("/p0"));
        for i in 1..MAX_PROJECTS {
            assert!(ws.add(project_at(&format!("/p{i}"))));
        }
        assert_eq!(ws.projects().len(), MAX_PROJECTS);
        let active_before = ws.active().repo_path.clone();

        assert!(!ws.add(project_at("/overflow")));

        assert_eq!(ws.projects().len(), MAX_PROJECTS);
        assert_eq!(ws.active().repo_path, active_before);
        assert!(ws.index_of_repo("/overflow").is_none());
    }

    #[test]
    fn 마지막_프로젝트는_닫을_수_없다() {
        let mut ws = Workspace::new(project_at("/a"));

        assert!(!ws.close_active());

        assert_eq!(paths(&ws), vec!["/a"]);
        assert_eq!(ws.active().repo_path, "/a");
    }

    #[test]
    fn 가운데_탭을_닫으면_뒤_탭이_활성이_된다() {
        let mut ws = Workspace::new(project_at("/a"));
        ws.add(project_at("/b"));
        ws.add(project_at("/c"));
        ws.switch(1);

        assert!(ws.close_active());

        assert_eq!(paths(&ws), vec!["/a", "/c"]);
        assert_eq!(ws.active().repo_path, "/c");
    }

    #[test]
    fn 마지막_탭을_닫으면_앞_탭이_활성이_된다() {
        let mut ws = Workspace::new(project_at("/a"));
        ws.add(project_at("/b"));

        assert!(ws.close_active());

        assert_eq!(paths(&ws), vec!["/a"]);
        assert_eq!(ws.active().repo_path, "/a");
    }

    #[test]
    fn 전환하면_이전_프로젝트의_대기중인_마우스_press가_버려진다() {
        let mut ws = Workspace::new(project_at("/a"));
        ws.add(project_at("/b"));
        ws.switch(0);
        ws.active_mut().pending_mouse_press = Some((1, crossterm::event::MouseButton::Left));

        ws.switch(1);

        // The release will be routed to /b, so /a's press can never be paired;
        // leaving it would let a later unrelated release match it.
        assert!(ws.projects()[0].pending_mouse_press.is_none());
    }

    #[test]
    fn 같은_인덱스로_전환하면_대기중인_press를_유지한다() {
        // A no-op switch must not disturb an in-flight press/release pair.
        let mut ws = Workspace::new(project_at("/a"));
        let press = Some((1, crossterm::event::MouseButton::Left));
        ws.active_mut().pending_mouse_press = press;

        ws.switch(0);

        assert_eq!(ws.active().pending_mouse_press, press);
    }

    #[test]
    fn 범위를_벗어난_전환은_활성을_바꾸지_않는다() {
        let mut ws = Workspace::new(project_at("/a"));
        ws.add(project_at("/b"));

        ws.switch(9);

        assert_eq!(ws.active().repo_path, "/b");
    }

    #[test]
    fn 열린_저장소는_경로로_찾을_수_있고_없으면_none이다() {
        let mut ws = Workspace::new(project_at("/a"));
        ws.add(project_at("/b"));

        assert_eq!(ws.index_of_repo("/a"), Some(0));
        assert_eq!(ws.index_of_repo("/b"), Some(1));
        assert_eq!(ws.index_of_repo("/nope"), None);
    }
}
