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
use crossterm::event::{KeyEvent, KeyModifiers};

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

    pub fn is_leader_key(&self, key: KeyEvent) -> bool {
        let relevant = KeyModifiers::CONTROL | KeyModifiers::ALT | KeyModifiers::SHIFT;
        key.code == self.leader.code
            && (key.modifiers & relevant) == (self.leader.modifiers & relevant)
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
mod tests {
    use super::*;
    use crate::app::tests::app_with_files;
    use crossterm::event::KeyCode;

    fn test_leader() -> KeyEvent {
        KeyEvent::new(KeyCode::Char('q'), KeyModifiers::CONTROL)
    }

    /// A workspace holding projects distinguished by `repo_path`.
    fn workspace_on(paths: &[&str]) -> Workspace {
        let mut ws = Workspace::new(test_leader());
        for p in paths {
            assert!(ws.add(project_at(p)));
        }
        ws
    }

    /// A project distinguishable from its siblings by `repo_path`, which is
    /// what the tab row labels and `index_of_repo` match on.
    fn project_at(path: &str) -> App {
        let mut app = app_with_files(vec!["a.rs"]);
        app.repo_path = path.to_string();
        app
    }

    fn workspace_from(project: App) -> Workspace {
        let mut ws = Workspace::new(test_leader());
        ws.add(project);
        ws
    }

    fn paths(ws: &Workspace) -> Vec<&str> {
        ws.projects().iter().map(|p| p.repo_path.as_str()).collect()
    }

    #[test]
    fn first_typed_char_replaces_the_prefilled_repo_path() {
        let mut ws = workspace_on(&["/repos/current"]);
        ws.start_repo_input();
        assert_eq!(ws.repo_input.buf, "/repos/current");

        for c in "/tmp".chars() {
            ws.repo_input_push(c);
        }

        assert_eq!(
            ws.repo_input.buf, "/tmp",
            "typing over an untouched prefill must replace it, not append"
        );
    }


    #[test]
    fn backspace_leaves_prefill_mode_without_dropping_the_path() {
        let mut ws = workspace_on(&["/repos/current"]);
        ws.start_repo_input();

        ws.repo_input_pop();
        assert_eq!(ws.repo_input.buf, "/repos/curren");
        ws.repo_input_push('t');
        assert_eq!(
            ws.repo_input.buf, "/repos/current",
            "after Backspace, typing must append to the surviving text"
        );
    }


    #[test]
    fn accepting_the_prefill_appends_instead_of_replacing() {
        let mut ws = workspace_on(&["/repos/current/"]);
        ws.start_repo_input();

        ws.repo_input_accept_prefill();
        for c in "src".chars() {
            ws.repo_input_push(c);
        }

        assert_eq!(ws.repo_input.buf, "/repos/current/src");
    }


    #[test]
    fn confirming_a_tilde_path_opens_the_home_relative_directory() {
        // The dialog never passes through a shell, so an unexpanded `~` would
        // be rejected as "no such directory".
        let home = dirs::home_dir().expect("a home directory");
        let mut ws = workspace_on(&["/repos/current"]);
        ws.start_repo_input();
        for c in "~".chars() {
            ws.repo_input_push(c);
        }

        let result = ws.confirm_repo_input();

        assert_eq!(
            result,
            RepoInputResult::Open(
                crate::git::resolve_repo_path(&home)
                    .to_string_lossy()
                    .to_string()
            )
        );
    }

    #[test]
    fn reopening_the_dialog_re_arms_the_prefill() {
        let mut ws = workspace_on(&["/repos/current"]);
        ws.start_repo_input();
        ws.repo_input_push('x');
        ws.cancel_repo_input();

        ws.start_repo_input();
        ws.repo_input_push('y');
        assert_eq!(ws.repo_input.buf, "y");
    }

    #[test]
    fn 빈_workspace는_활성_프로젝트가_없다() {
        let ws = Workspace::new(test_leader());

        assert!(ws.active().is_none());
        assert!(ws.projects().is_empty());
    }

    #[test]
    fn 닫은_프로젝트_기억은_상한을_넘지_않는다() {
        // A long-lived process that opens and closes many repositories would
        // otherwise hold every session it had ever seen — and rescan that
        // history on every save.
        let mut ws = Workspace::new(test_leader());
        for i in 0..MAX_REMEMBERED + 5 {
            ws.add(project_at(&format!("/w/p{i}")));
            ws.close_active();
        }

        assert_eq!(ws.remembered.len(), MAX_REMEMBERED);
        // The most recently closed survives; the oldest is gone.
        assert!(ws.session_for(&format!("/w/p{}", MAX_REMEMBERED + 4)).is_some());
        assert!(ws.session_for("/w/p0").is_none());
    }

    #[test]
    fn 마지막_탭을_닫으면_빈_상태가_된다() {
        // Quitting is no longer the only way out of the last project: the
        // empty screen is a real state nightcrow also starts in.
        let mut ws = workspace_on(&["/a"]);

        assert!(ws.close_active());

        assert!(ws.active().is_none());
        assert!(!ws.close_active(), "nothing left to close");
    }

    #[test]
    fn 프로젝트가_없으면_다이얼로그가_빈_상태로_열린다() {
        let mut ws = Workspace::new(test_leader());

        ws.start_repo_input();

        assert!(ws.repo_input.active);
        assert_eq!(ws.repo_input.buf, "", "no project to prefill from");
    }

    #[test]
    fn 프로젝트를_열면_빈_화면_공지가_사라진다() {
        // Otherwise a stale rejection would reappear the moment the last tab
        // was closed again, long after it stopped being true.
        let mut ws = Workspace::new(test_leader());
        ws.raise_notice(NoticeKind::RepoInput, "no such directory");

        ws.add(project_at("/a"));
        ws.close_active();

        assert!(ws.active().is_none(), "back to the empty screen");
        assert!(ws.empty_notice().is_none());
    }

    #[test]
    fn 프로젝트가_없으면_공지가_workspace에_남는다() {
        let mut ws = Workspace::new(test_leader());

        ws.raise_notice(NoticeKind::RepoInput, "no such directory");

        assert_eq!(
            ws.empty_notice().map(|n| n.text.as_str()),
            Some("no such directory")
        );
        ws.clear_notice(NoticeKind::RepoInput);
        assert!(ws.empty_notice().is_none());
    }

    #[test]
    fn 새_workspace는_프로젝트_하나를_활성으로_갖는다() {
        let ws = workspace_from(app_with_files(vec!["a.rs"]));

        assert_eq!(ws.projects().len(), 1);
        assert_eq!(ws.active().unwrap().repo_path, ".");
    }

    #[test]
    fn 프로젝트를_추가하면_끝에_붙고_활성이_된다() {
        let mut ws = workspace_from(project_at("/a"));

        assert!(ws.add(project_at("/b")));

        assert_eq!(paths(&ws), vec!["/a", "/b"]);
        assert_eq!(ws.active().unwrap().repo_path, "/b");
    }

    #[test]
    fn 상한에_도달하면_추가를_거부하고_활성을_유지한다() {
        let mut ws = workspace_from(project_at("/p0"));
        for i in 1..MAX_PROJECTS {
            assert!(ws.add(project_at(&format!("/p{i}"))));
        }
        assert_eq!(ws.projects().len(), MAX_PROJECTS);
        let active_before = ws.active().unwrap().repo_path.clone();

        assert!(!ws.add(project_at("/overflow")));

        assert_eq!(ws.projects().len(), MAX_PROJECTS);
        assert_eq!(ws.active().unwrap().repo_path, active_before);
        assert!(ws.index_of_repo("/overflow").is_none());
    }

    #[test]
    fn 가운데_탭을_닫으면_뒤_탭이_활성이_된다() {
        let mut ws = workspace_from(project_at("/a"));
        ws.add(project_at("/b"));
        ws.add(project_at("/c"));
        ws.switch(1);

        assert!(ws.close_active());

        assert_eq!(paths(&ws), vec!["/a", "/c"]);
        assert_eq!(ws.active().unwrap().repo_path, "/c");
    }

    #[test]
    fn 마지막_탭을_닫으면_앞_탭이_활성이_된다() {
        let mut ws = workspace_from(project_at("/a"));
        ws.add(project_at("/b"));

        assert!(ws.close_active());

        assert_eq!(paths(&ws), vec!["/a"]);
        assert_eq!(ws.active().unwrap().repo_path, "/a");
    }

    #[test]
    fn 프로젝트를_추가해도_이전_프로젝트의_press가_버려진다() {
        // `add` makes the new project active, so the outgoing project's press
        // is released in place — same reasoning as `switch`.
        let mut ws = workspace_on(&["/a"]);
        ws.active_mut().unwrap().pending_mouse_press = Some((1, crossterm::event::MouseButton::Left, 1, 1));

        ws.add(project_at("/b"));

        assert!(ws.projects()[0].pending_mouse_press.is_none());
    }

    #[test]
    fn 전환하면_이전_프로젝트의_대기중인_마우스_press가_버려진다() {
        let mut ws = workspace_from(project_at("/a"));
        ws.add(project_at("/b"));
        ws.switch(0);
        ws.active_mut().unwrap().pending_mouse_press = Some((1, crossterm::event::MouseButton::Left, 1, 1));

        ws.switch(1);

        // /a's press can never be paired now, so it is released where it
        // happened rather than left to match an unrelated release later.
        assert!(ws.projects()[0].pending_mouse_press.is_none());
    }

    #[test]
    fn 같은_인덱스로_전환하면_대기중인_press를_유지한다() {
        // A no-op switch must not disturb an in-flight press/release pair.
        let mut ws = workspace_from(project_at("/a"));
        let press = Some((1, crossterm::event::MouseButton::Left, 1, 1));
        ws.active_mut().unwrap().pending_mouse_press = press;

        ws.switch(0);

        assert_eq!(ws.active().unwrap().pending_mouse_press, press);
    }

    #[test]
    fn 범위를_벗어난_전환은_활성을_바꾸지_않는다() {
        let mut ws = workspace_from(project_at("/a"));
        ws.add(project_at("/b"));

        ws.switch(9);

        assert_eq!(ws.active().unwrap().repo_path, "/b");
    }

    #[test]
    fn 열린_저장소는_경로로_찾을_수_있고_없으면_none이다() {
        let mut ws = workspace_from(project_at("/a"));
        ws.add(project_at("/b"));

        assert_eq!(ws.index_of_repo("/a"), Some(0));
        assert_eq!(ws.index_of_repo("/b"), Some(1));
        assert_eq!(ws.index_of_repo("/nope"), None);
    }
}
