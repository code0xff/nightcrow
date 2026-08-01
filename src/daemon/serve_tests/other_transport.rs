//! What an attached client is told about changes it did not make.
//!
//! The session is one thing behind two transports, so a repository opened or
//! closed over HTTP has to reach a terminal that asked nothing — and reach it
//! with the same answer, including which project the close puts in front.

use super::harness::{Client, daemon, repo_paths, resolved};
use crate::daemon::protocol::ClientMessage;

#[test]
fn a_repository_opened_through_the_browser_reaches_a_client_that_asked_nothing() {
    // The session has two front doors. A change through the other one wakes
    // nothing on an attach socket, so without the watcher a client would sit on
    // a tab list that had quietly gone stale — which is the one thing a shared
    // session must not do.
    let (repo, path) = crate::test_util::make_repo();
    let dir = tempfile::TempDir::new().unwrap();
    let daemon = daemon(&dir, &[]);
    let mut client = Client::attach(daemon.path());

    // Straight against the session, the way the viewer's HTTP handler does it —
    // this client's connection is not involved at all.
    crate::session::open_repo(daemon.state(), &path).expect("opens");

    assert_eq!(repo_paths(&client.next_repos()), vec![resolved(&path)]);
    drop(repo);
}

#[test]
fn a_repository_closed_through_the_browser_reaches_it_too() {
    let (repo, path) = crate::test_util::make_repo();
    let dir = tempfile::TempDir::new().unwrap();
    let daemon = daemon(&dir, std::slice::from_ref(&path));
    let mut client = Client::attach(daemon.path());
    let id = client.repo_ids().pop().expect("one repository is open");

    crate::session::close_repo(daemon.state(), &id).expect("closes");

    assert!(repo_paths(&client.next_repos()).is_empty());
    drop(repo);
}

/// The successor reaches an attached client too, through the set it is sent.
///
/// Every other test of this closes over HTTP, and the daemon is the surface
/// where `active_repo`'s first-served fallback lives — the one that used to
/// answer a close with the first tab. Without this the watcher could go back to
/// advertising that fallback and every HTTP test would still pass.
#[test]
fn closing_the_project_in_front_tells_an_attached_client_its_successor() {
    let (repo_a, a) = crate::test_util::make_repo();
    let (repo_b, b) = crate::test_util::make_repo();
    let (repo_c, c) = crate::test_util::make_repo();
    let dir = tempfile::TempDir::new().unwrap();
    let daemon = daemon(&dir, &[a, b, c]);
    let mut client = Client::attach(daemon.path());
    let ids = client.repo_ids();

    client.send(ClientMessage::FocusRepo {
        repo: ids[1].clone(),
    });
    assert_eq!(client.next_active(), Some(ids[1].clone()));

    crate::session::close_repo(daemon.state(), &ids[1]).expect("closes");

    // The tab after the closed one is what the session settles on.
    client.wait_for_active(Some(&ids[2]));
    drop((repo_a, repo_b, repo_c));
}
