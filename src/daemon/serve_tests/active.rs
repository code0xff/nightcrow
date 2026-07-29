//! Which project the session has in front.
//!
//! Shared, not per-client: every client renders the one the session names, and
//! switching is a request. What stays local is everything inside a project.

use super::harness::*;
use crate::daemon::protocol::{ClientMessage, ServerMessage};

#[test]
fn the_session_names_the_project_in_front_and_opening_one_focuses_it() {
    // Which tab is in front is shared, so it comes with the set. Opening is also
    // a statement about where the client wants to be — leaving the focus behind
    // would put the tab someone just asked for in the background.
    let (repo, path) = crate::test_util::make_repo();
    let (other, other_path) = crate::test_util::make_repo();
    let dir = tempfile::TempDir::new().unwrap();
    let daemon = daemon(&dir, std::slice::from_ref(&path));
    let mut client = Client::attach(daemon.path());

    client.send(ClientMessage::OpenRepo {
        path: other_path.clone(),
    });

    let ids = client.repo_ids();
    let opened = ids
        .iter()
        .find(|id| id.as_str() != ids[0])
        .cloned()
        .expect("two repositories are open");
    client.send(ClientMessage::ListRepos);
    assert_eq!(client.next_active().as_deref(), Some(opened.as_str()));
    drop((repo, other));
}

#[test]
fn focusing_a_project_reaches_the_other_clients() {
    // The point of sharing it: two clients on one session are looking at the
    // same project, not one each.
    let (repo, path) = crate::test_util::make_repo();
    let (other, other_path) = crate::test_util::make_repo();
    let dir = tempfile::TempDir::new().unwrap();
    let daemon = daemon(&dir, &[path.clone(), other_path.clone()]);
    let mut switcher = Client::attach(daemon.path());
    let mut watcher = Client::attach(daemon.path());
    let ids = switcher.repo_ids();
    let second = ids[1].clone();

    switcher.send(ClientMessage::FocusRepo {
        repo: second.clone(),
    });

    assert_eq!(
        watcher.next_active().as_deref(),
        Some(second.as_str()),
        "the other client follows"
    );
    drop((repo, other));
}

#[test]
fn a_session_that_has_never_been_focused_still_names_a_project() {
    // Otherwise every client would pick for itself, which is the divergence
    // sharing this is meant to remove.
    let (repo, path) = crate::test_util::make_repo();
    let dir = tempfile::TempDir::new().unwrap();
    let daemon = daemon(&dir, std::slice::from_ref(&path));
    let mut client = Client::attach(daemon.path());

    client.send(ClientMessage::ListRepos);

    let ids = client.repo_ids();
    client.send(ClientMessage::ListRepos);
    assert_eq!(client.next_active(), Some(ids[0].clone()));
    drop(repo);
}

#[test]
fn focusing_a_repository_the_session_does_not_have_is_refused() {
    // The asker is waiting for that tab to come forward and never will.
    let dir = tempfile::TempDir::new().unwrap();
    let daemon = daemon(&dir, &[]);
    let mut client = Client::attach(daemon.path());

    let refused = client.ask(ClientMessage::FocusRepo {
        repo: "r-nonexistent".into(),
    });

    assert!(matches!(refused, ServerMessage::Error { .. }));
}
