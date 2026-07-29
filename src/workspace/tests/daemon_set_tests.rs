//! The primitives an attached client uses to adopt the set the daemon reports.
//!
//! Closing and reordering here are not driven by the person at the keyboard —
//! they answer a change another client made — so neither may go through the
//! active tab, and neither may move which project the user is looking at.

use super::common::*;

fn paths(ws: &crate::workspace::Workspace) -> Vec<String> {
    ws.projects().iter().map(|p| p.repo_path.clone()).collect()
}

#[test]
fn closing_a_repo_that_is_not_active_leaves_the_active_one_alone() {
    // The usual case when adopting: another client closed something the person
    // here is not looking at.
    let mut ws = workspace_on(&["/a", "/b", "/c"]);
    ws.switch(2);

    assert!(ws.close_repo("/a"));

    assert_eq!(paths(&ws), vec!["/b", "/c"]);
    assert_eq!(
        ws.active().unwrap().repo_path,
        "/c",
        "the active project must not shift onto its neighbour"
    );
}

#[test]
fn closing_the_active_repo_falls_back_to_a_neighbour() {
    let mut ws = workspace_on(&["/a", "/b", "/c"]);
    ws.switch(1);

    assert!(ws.close_repo("/b"));

    assert_eq!(paths(&ws), vec!["/a", "/c"]);
    assert_eq!(ws.active().unwrap().repo_path, "/c");
}

#[test]
fn closing_the_last_repo_leaves_an_empty_workspace() {
    let mut ws = workspace_on(&["/a"]);

    assert!(ws.close_repo("/a"));

    assert!(ws.projects().is_empty());
    assert!(ws.active().is_none());
}

#[test]
fn closing_a_repo_that_is_not_open_reports_it_rather_than_closing_something_else() {
    let mut ws = workspace_on(&["/a", "/b"]);

    assert!(!ws.close_repo("/gone"));

    assert_eq!(paths(&ws), vec!["/a", "/b"]);
}

#[test]
fn a_closed_repo_keeps_its_view_state_for_when_it_comes_back() {
    // Same guarantee closing by hand has: the daemon may reopen a repository,
    // and its selection should come back with it rather than the state from
    // whenever the process last shut down.
    let mut ws = workspace_on(&["/a", "/b"]);

    ws.close_repo("/a");

    assert!(
        ws.session_for("/a").is_some(),
        "the closed project's view state must be remembered"
    );
}

#[test]
fn reordering_puts_the_tabs_in_the_given_order() {
    let mut ws = workspace_on(&["/a", "/b", "/c"]);

    ws.reorder_to(&["/c", "/a", "/b"]);

    assert_eq!(paths(&ws), vec!["/c", "/a", "/b"]);
}

#[test]
fn reordering_keeps_the_same_project_active() {
    // Tracked by path, not index: the whole point is that the indices moved,
    // so keeping the index would leave the user looking at a different repo.
    let mut ws = workspace_on(&["/a", "/b", "/c"]);
    ws.switch(0);

    ws.reorder_to(&["/c", "/b", "/a"]);

    assert_eq!(ws.active().unwrap().repo_path, "/a");
    assert_eq!(ws.active_index(), 2);
}

#[test]
fn an_order_naming_a_repo_that_is_not_open_skips_it() {
    // The order can race a close on another client.
    let mut ws = workspace_on(&["/a", "/b"]);

    ws.reorder_to(&["/gone", "/b", "/a"]);

    assert_eq!(paths(&ws), vec!["/b", "/a"]);
}

#[test]
fn an_order_that_omits_an_open_repo_keeps_it_rather_than_dropping_it() {
    // Reordering must never close anything — that is the other operation, and
    // a tab silently disappearing on a reorder would take its terminals along.
    let mut ws = workspace_on(&["/a", "/b", "/c"]);

    ws.reorder_to(&["/c"]);

    assert_eq!(paths(&ws), vec!["/c", "/a", "/b"]);
}

#[test]
fn an_empty_order_leaves_the_tabs_as_they_were() {
    let mut ws = workspace_on(&["/a", "/b"]);

    ws.reorder_to(&[]);

    assert_eq!(paths(&ws), vec!["/a", "/b"]);
}

#[test]
fn recording_an_id_names_the_repository_it_was_given_for() {
    // The id is how this client asks the daemon to close a repository, so it
    // must land on the right tab.
    let mut ws = workspace_on(&["/a", "/b"]);

    ws.set_repo_id("/b", "r7");

    assert_eq!(ws.projects()[1].repo_id.as_deref(), Some("r7"));
    assert_eq!(ws.projects()[0].repo_id, None);
}

#[test]
fn recording_an_id_for_a_repo_that_is_not_open_changes_nothing() {
    let mut ws = workspace_on(&["/a"]);

    ws.set_repo_id("/gone", "r7");

    assert_eq!(ws.projects()[0].repo_id, None);
}
