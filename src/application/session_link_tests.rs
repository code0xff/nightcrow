use super::{cycle_target, focus_repo, notify_repo};
use crate::app::App;
use crate::app::tests::app_with_files;
use crate::workspace::Workspace;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

fn project_at(path: &str) -> App {
    let mut app = app_with_files(vec!["a.rs"]);
    app.git.repo_path = path.to_string();
    app
}

/// A workspace with a tab per path, each carrying the catalog id the daemon
/// would have given it (`r-/a` for `/a`), since that is what a client names a
/// repository by.
fn workspace_on(paths: &[&str]) -> Workspace {
    let mut ws = Workspace::new(KeyEvent::new(KeyCode::Char('f'), KeyModifiers::CONTROL));
    for path in paths {
        assert!(ws.add(project_at(path)));
        ws.set_repo_id(path, &id_of(path));
    }
    ws
}

fn id_of(path: &str) -> String {
    format!("r-{path}")
}

#[test]
fn the_client_shows_whichever_project_the_session_names() {
    // Which tab is in front is shared, so this is the whole of how a client
    // switches: it renders the repository the session says is active, whether
    // that came from its own keystroke or another client's.
    let mut ws = workspace_on(&["/a", "/b"]);
    ws.switch(1);

    assert!(focus_repo(&mut ws, &id_of("/a")));

    assert_eq!(ws.active_index(), 0);
    assert_eq!(ws.projects().len(), 2, "and does not open a second tab");
}

#[test]
fn a_project_this_client_has_no_tab_for_yet_leaves_the_tabs_alone() {
    // The session can name a repository in the beat before this client has
    // built its tab. A miss must be inert, not a jump to tab zero.
    let mut ws = workspace_on(&["/a", "/b"]);
    ws.switch(1);

    assert!(!focus_repo(&mut ws, "r-elsewhere"));

    assert_eq!(ws.active_index(), 1);
}

#[test]
fn focusing_in_an_empty_workspace_is_inert() {
    let mut ws = workspace_on(&[]);

    assert!(!focus_repo(&mut ws, &id_of("/a")));

    assert!(ws.active().is_none());
}

#[test]
fn a_terminal_refusal_lands_on_the_tab_of_the_repository_it_came_from() {
    // The client subscribes to every open repository's terminals, so a refusal
    // can be about one the user is not looking at. Putting it on whichever tab
    // is in front would name the wrong project.
    let mut ws = workspace_on(&["/a", "/b"]);
    ws.set_repo_id("/a", "r-a");
    ws.set_repo_id("/b", "r-b");
    ws.switch(1);

    notify_repo(&mut ws, "r-a", "terminal limit reached".into());

    let notice = ws.projects()[0].notice.as_ref().expect("raised on /a");
    assert_eq!(notice.kind, crate::app::NoticeKind::Terminal);
    assert_eq!(notice.text, "terminal limit reached");
    assert!(
        ws.projects()[1].notice.is_none(),
        "and not on the front tab"
    );
}

#[test]
fn a_refusal_for_a_repository_with_no_tab_is_still_shown() {
    // A repository can be closed here a beat before its hub answers. The
    // message is worth more on the wrong tab than nowhere.
    let mut ws = workspace_on(&["/a"]);
    ws.set_repo_id("/a", "r-a");

    notify_repo(&mut ws, "r-gone", "could not start a terminal".into());

    assert!(ws.projects()[0].notice.is_some());
}

#[test]
fn stepping_forward_from_the_last_tab_wraps_to_the_first() {
    let mut ws = workspace_on(&["/a", "/b", "/c"]);
    ws.switch(2);

    assert_eq!(cycle_target(&ws, true), Some(id_of("/a")));
}

#[test]
fn stepping_backward_from_the_first_tab_wraps_to_the_last() {
    let mut ws = workspace_on(&["/a", "/b", "/c"]);
    ws.switch(0);

    assert_eq!(cycle_target(&ws, false), Some(id_of("/c")));
}

#[test]
fn stepping_between_two_tabs_alternates_in_both_directions() {
    let mut ws = workspace_on(&["/a", "/b"]);
    ws.switch(0);

    assert_eq!(cycle_target(&ws, true), Some(id_of("/b")));
    assert_eq!(cycle_target(&ws, false), Some(id_of("/b")));
}

#[test]
fn stepping_with_fewer_than_two_tabs_asks_for_nothing() {
    // One tab is already the destination in either direction, and an empty
    // workspace has none — asking the daemon to focus what is in front would
    // be a broadcast that changes nothing for every client.
    assert_eq!(cycle_target(&workspace_on(&[]), true), None);
    assert_eq!(cycle_target(&workspace_on(&["/a"]), true), None);
    assert_eq!(cycle_target(&workspace_on(&["/a"]), false), None);
}

#[test]
fn stepping_onto_a_tab_the_session_has_not_named_yet_asks_for_nothing() {
    // A client names a repository by catalog id, so a tab still waiting for
    // one cannot be asked for — the same early-out as closing an unnamed tab.
    let mut ws = Workspace::new(KeyEvent::new(KeyCode::Char('f'), KeyModifiers::CONTROL));
    assert!(ws.add(project_at("/a")));
    assert!(ws.add(project_at("/b")));
    ws.set_repo_id("/a", &id_of("/a"));

    ws.switch(0);
    assert_eq!(cycle_target(&ws, true), None);
    ws.switch(1);
    assert_eq!(cycle_target(&ws, true), Some(id_of("/a")));
}

#[test]
fn resolving_a_step_leaves_the_front_tab_where_it_is() {
    // Switching is a request: the tab moves only when the daemon rebroadcasts
    // the set, so resolving the target must not move it optimistically.
    let mut ws = workspace_on(&["/a", "/b", "/c"]);
    ws.switch(1);

    let _ = cycle_target(&ws, true);
    let _ = cycle_target(&ws, false);

    assert_eq!(ws.active_index(), 1);
}
