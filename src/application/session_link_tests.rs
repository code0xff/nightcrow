use super::focus_if_open;
use crate::app::App;
use crate::app::tests::app_with_files;
use crate::workspace::Workspace;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

fn project_at(path: &str) -> App {
    let mut app = app_with_files(vec!["a.rs"]);
    app.repo_path = path.to_string();
    app
}

fn workspace_on(paths: &[&str]) -> Workspace {
    let mut ws = Workspace::new(KeyEvent::new(KeyCode::Char('f'), KeyModifiers::CONTROL));
    for path in paths {
        assert!(ws.add(project_at(path)));
    }
    ws
}

#[test]
fn asking_for_a_repo_another_tab_already_holds_moves_to_it() {
    // What the dialog did before the daemon owned the tabs: opening a
    // repository that is already open focuses it. Through a daemon the open is
    // a request whose answer is the whole set, which says nothing about who
    // asked — so the focus has to be applied here or the key looks inert.
    let mut ws = workspace_on(&["/a", "/b"]);
    ws.switch(1);

    assert!(focus_if_open(&mut ws, "/a"));

    assert_eq!(ws.active_index(), 0);
    assert_eq!(ws.projects().len(), 2, "and does not open a second tab");
}

#[test]
fn asking_for_a_repo_that_is_not_open_leaves_the_tabs_alone() {
    // The daemon resolves a path to its worktree root, so what comes back may
    // not match what was typed. A miss must be inert, not a jump to tab zero.
    let mut ws = workspace_on(&["/a", "/b"]);
    ws.switch(1);

    assert!(!focus_if_open(&mut ws, "/elsewhere"));

    assert_eq!(ws.active_index(), 1);
}

#[test]
fn focusing_in_an_empty_workspace_is_inert() {
    let mut ws = workspace_on(&[]);

    assert!(!focus_if_open(&mut ws, "/a"));

    assert!(ws.active().is_none());
}
