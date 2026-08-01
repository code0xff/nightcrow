//! Which project a close puts in front.
//!
//! Beside `active_repo`, whose preference this writes: the successor is not a
//! separate piece of state, it is that one being set by a close rather than by
//! somebody selecting a tab.

use super::active_repo::{select, served_active, served_ids, server_at};
use super::{delete, login};
use crate::test_util::make_repo;

/// Closing the project in front moves to the next tab, not to the first one.
///
/// Nothing used to decide this: the stored path stopped resolving and
/// `active_repo`'s fallback — meant for a session that has focused nothing yet
/// — answered with the first repository, so closing the third of four sent
/// everyone to the first.
#[test]
fn closing_the_active_project_moves_to_the_one_after_it() {
    let prefs = tempfile::TempDir::new().unwrap();
    let (_a, a) = make_repo();
    let (_b, b) = make_repo();
    let (_c, c) = make_repo();
    let server = server_at(prefs.path(), &[a, b, c]);
    let token = login(server.addr());
    let ids = served_ids(server.addr(), &token);
    select(server.addr(), &ids[1], &token);

    delete(
        server.addr(),
        &format!("/api/repos?repo={}", ids[1]),
        Some(&token),
    );

    assert_eq!(
        served_active(server.addr(), &token),
        serde_json::json!(ids[2]),
        "the tab after the closed one takes the front"
    );
}

#[test]
fn closing_the_last_project_moves_to_the_one_before_it() {
    let prefs = tempfile::TempDir::new().unwrap();
    let (_a, a) = make_repo();
    let (_b, b) = make_repo();
    let server = server_at(prefs.path(), &[a, b]);
    let token = login(server.addr());
    let ids = served_ids(server.addr(), &token);
    select(server.addr(), &ids[1], &token);

    delete(
        server.addr(),
        &format!("/api/repos?repo={}", ids[1]),
        Some(&token),
    );

    assert_eq!(
        served_active(server.addr(), &token),
        serde_json::json!(ids[0]),
        "with nothing after it, the tab before takes the front"
    );
}

#[test]
fn closing_a_background_project_leaves_the_front_alone() {
    // The focus is a place the person is, not a place the set decides.
    let prefs = tempfile::TempDir::new().unwrap();
    let (_a, a) = make_repo();
    let (_b, b) = make_repo();
    let (_c, c) = make_repo();
    let server = server_at(prefs.path(), &[a, b, c]);
    let token = login(server.addr());
    let ids = served_ids(server.addr(), &token);
    select(server.addr(), &ids[2], &token);

    delete(
        server.addr(),
        &format!("/api/repos?repo={}", ids[0]),
        Some(&token),
    );

    assert_eq!(
        served_active(server.addr(), &token),
        serde_json::json!(ids[2]),
        "closing a project behind the front must not move it"
    );
}

#[test]
fn closing_the_only_project_leaves_nothing_in_front() {
    let prefs = tempfile::TempDir::new().unwrap();
    let (_a, a) = make_repo();
    let server = server_at(prefs.path(), &[a]);
    let token = login(server.addr());
    let ids = served_ids(server.addr(), &token);
    // Selected first, or this would watch a null that was already null and pass
    // whether or not the close did anything.
    select(server.addr(), &ids[0], &token);
    assert_eq!(
        served_active(server.addr(), &token),
        serde_json::json!(ids[0])
    );

    let closed = delete(
        server.addr(),
        &format!("/api/repos?repo={}", ids[0]),
        Some(&token),
    );

    assert!(closed.starts_with("HTTP/1.1 200"), "got: {closed}");
    assert!(
        served_ids(server.addr(), &token).is_empty(),
        "the project must actually be gone"
    );
    assert_eq!(
        served_active(server.addr(), &token),
        serde_json::Value::Null,
        "an empty session has no project in front"
    );
}
