use crate::app::App;
use crate::app::tests::app_with_files;
use crate::workspace::Workspace;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

pub(super) fn test_leader() -> KeyEvent {
    KeyEvent::new(KeyCode::Char('f'), KeyModifiers::CONTROL)
}

/// A workspace holding projects distinguished by `repo_path`.
pub(super) fn workspace_on(paths: &[&str]) -> Workspace {
    let mut ws = Workspace::new(test_leader());
    for p in paths {
        assert!(ws.add(project_at(p)));
    }
    ws
}

/// A project distinguishable from its siblings by `repo_path`, which is
/// what the tab row labels and `index_of_repo` match on.
pub(super) fn project_at(path: &str) -> App {
    let mut app = app_with_files(vec!["a.rs"]);
    app.repo_path = path.to_string();
    app
}

pub(super) fn workspace_from(project: App) -> Workspace {
    let mut ws = Workspace::new(test_leader());
    ws.add(project);
    ws
}

pub(super) fn paths(ws: &Workspace) -> Vec<&str> {
    ws.projects().iter().map(|p| p.repo_path.as_str()).collect()
}
