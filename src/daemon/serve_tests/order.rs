//! The order the session keeps its repositories in.
//!
//! Shared like the set and the project in front: a client asks for a whole new
//! order by catalog id and adopts what comes back, rather than rearranging its
//! own tabs and hoping the others agree.

use super::harness::*;
use crate::daemon::protocol::{ClientMessage, ServerMessage};

/// Wait until the session advertises `want` as its order.
///
/// Not the *next* set, for the same reason `wait_until_active` is not: the
/// watcher speaks on a tick, so a snapshot taken between the request and the
/// change is true of that instant without being the answer.
fn wait_until_order(client: &mut Client, want: &[String]) {
    let mut last = Vec::new();
    loop {
        let Some(ServerMessage::Repos { repos, .. }) = client.try_next_repos() else {
            panic!("the session settled on {last:?} rather than {want:?}");
        };
        last = repos.into_iter().map(|repo| repo.id).collect::<Vec<_>>();
        if last == want {
            return;
        }
    }
}

#[test]
fn reordering_reaches_the_other_clients_and_keeps_the_same_project_in_front() {
    // The whole reason order is session state: two clients on one session see
    // the same strip. And the front tab is tracked separately from the order, so
    // swapping the two tabs must not hand the front to the other one — which is
    // the case that needs saying while nothing has been focused, because the
    // front then falls back to whichever repository is served first.
    let (repo, path) = crate::test_util::make_repo();
    let (other, other_path) = crate::test_util::make_repo();
    let dir = tempfile::TempDir::new().unwrap();
    let daemon = daemon(&dir, &[path.clone(), other_path.clone()]);
    let mut mover = Client::attach(daemon.path());
    let mut watcher = Client::attach(daemon.path());
    let ids = mover.repo_ids();
    let (first, second) = (ids[0].clone(), ids[1].clone());
    // Asked rather than assumed: a session that has never been focused still
    // names a project, and the assertion below is about that one staying put.
    mover.send(ClientMessage::ListRepos);
    assert_eq!(mover.next_active().as_deref(), Some(first.as_str()));

    mover.send(ClientMessage::ReorderRepos {
        order: vec![second.clone(), first.clone()],
    });

    wait_until_order(&mut watcher, &[second, first.clone()]);
    watcher.send(ClientMessage::ListRepos);
    assert_eq!(
        watcher.next_active().as_deref(),
        Some(first.as_str()),
        "the front tab follows its repository, not its slot"
    );
    drop((repo, other));
}

#[test]
fn an_order_naming_a_repository_the_session_does_not_have_drops_only_that_id() {
    // The only way to send one is to have raced a close on another client, so
    // it is skipped rather than refused — and the repositories that are still
    // open must survive it, since reordering never closes anything.
    let (repo, path) = crate::test_util::make_repo();
    let (other, other_path) = crate::test_util::make_repo();
    let dir = tempfile::TempDir::new().unwrap();
    let daemon = daemon(&dir, &[path.clone(), other_path.clone()]);
    let mut client = Client::attach(daemon.path());
    let ids = client.repo_ids();
    let (first, second) = (ids[0].clone(), ids[1].clone());

    client.send(ClientMessage::ReorderRepos {
        order: vec!["r-gone".into(), second.clone(), first.clone()],
    });

    wait_until_order(&mut client, &[second, first]);
    drop((repo, other));
}

#[test]
fn reordering_keeps_a_focused_project_in_front_after_its_slot_changes() {
    // The other branch of the same rule: once a project has been focused the
    // front is on file by path, and moving its slot must leave that alone.
    let (repo, path) = crate::test_util::make_repo();
    let (other, other_path) = crate::test_util::make_repo();
    let dir = tempfile::TempDir::new().unwrap();
    let daemon = daemon(&dir, &[path.clone(), other_path.clone()]);
    let mut client = Client::attach(daemon.path());
    let ids = client.repo_ids();
    let (first, second) = (ids[0].clone(), ids[1].clone());
    client.send(ClientMessage::FocusRepo {
        repo: second.clone(),
    });
    client.wait_until_active(Some(&second));

    client.send(ClientMessage::ReorderRepos {
        order: vec![second.clone(), first],
    });

    wait_until_order(&mut client, &ids.iter().rev().cloned().collect::<Vec<_>>());
    client.send(ClientMessage::ListRepos);
    assert_eq!(client.next_active().as_deref(), Some(second.as_str()));
    drop((repo, other));
}
