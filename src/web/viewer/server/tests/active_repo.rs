//! The project a client last selected, remembered server-side so a reload —
//! or another device — opens where the user left off.

use super::{body_of, delete, get, login, post, server_with};
use crate::test_util::make_repo;

/// Ids of the served repositories, in tab order.
fn served_ids(addr: std::net::SocketAddr, token: &str) -> Vec<String> {
    let list = get(addr, "/api/repos", Some(token));
    let value: serde_json::Value = serde_json::from_str(body_of(&list)).unwrap();
    value["repos"]
        .as_array()
        .unwrap()
        .iter()
        .map(|repo| repo["id"].as_str().unwrap().to_string())
        .collect()
}

fn served_active(addr: std::net::SocketAddr, token: &str) -> serde_json::Value {
    let list = get(addr, "/api/repos", Some(token));
    let value: serde_json::Value = serde_json::from_str(body_of(&list)).unwrap();
    value["active_repo"].clone()
}

fn select(addr: std::net::SocketAddr, id: &str, token: &str) -> String {
    post(
        addr,
        "/api/prefs",
        &format!("{{\"active_repo\":\"{id}\"}}"),
        Some(token),
    )
}

fn server_at(prefs_dir: &std::path::Path, paths: &[String]) -> super::ViewerServer {
    server_with(
        paths,
        crate::config::AgentIndicatorConfig::default(),
        Some(prefs_dir),
    )
}

#[test]
fn a_selected_project_is_served_back_to_every_later_client() {
    let prefs = tempfile::TempDir::new().unwrap();
    let (_a, a) = make_repo();
    let (_b, b) = make_repo();
    let server = server_at(prefs.path(), &[a, b]);
    let token = login(server.addr());
    let ids = served_ids(server.addr(), &token);

    let stored = select(server.addr(), &ids[1], &token);
    assert!(stored.starts_with("HTTP/1.1 200"), "got: {stored}");

    // The echo and the next bootstrap agree, so a second device opens the tab
    // this one chose without having chosen it.
    let echoed: serde_json::Value = serde_json::from_str(body_of(&stored)).unwrap();
    assert_eq!(echoed["active_repo"], ids[1].as_str());
    assert_eq!(served_active(server.addr(), &token), ids[1].as_str());
}

#[test]
fn nothing_is_active_until_a_client_selects_a_project() {
    let prefs = tempfile::TempDir::new().unwrap();
    let (_a, a) = make_repo();
    let server = server_at(prefs.path(), &[a]);
    let token = login(server.addr());

    // Null rather than the first tab: choosing the fallback is the client's
    // job, and it already has the list to choose from.
    assert_eq!(
        served_active(server.addr(), &token),
        serde_json::Value::Null
    );
}

#[test]
fn a_selection_survives_a_restart_that_renumbers_the_ids() {
    // The reason the path is stored and not the id: ids are handed out in
    // catalog order and only live as long as the process, so a restart that
    // opens the projects the other way round moves them. Storing an id would
    // silently resurrect it as *the other* project.
    let prefs = tempfile::TempDir::new().unwrap();
    let (_a, a) = make_repo();
    let (_b, b) = make_repo();

    let first = server_at(prefs.path(), &[a.clone(), b.clone()]);
    let token = login(first.addr());
    let ids = served_ids(first.addr(), &token);
    select(first.addr(), &ids[1], &token);
    drop(first);

    let second = server_at(prefs.path(), &[b, a]);
    let token = login(second.addr());
    let reordered = served_ids(second.addr(), &token);

    // Same repository, now the *first* tab and a different id.
    assert_eq!(served_active(second.addr(), &token), reordered[0].as_str());
}

#[test]
fn a_remembered_project_that_is_no_longer_served_reports_nothing_active() {
    // Not a close — closing names the neighbour (below). This is the entry a
    // file outlives: a session started without a project its preferences still
    // point at, because the directory went away or it was opened elsewhere.
    let prefs = tempfile::TempDir::new().unwrap();
    let (_a, a) = make_repo();
    let (_b, b) = make_repo();
    {
        let server = server_at(prefs.path(), &[a.clone(), b]);
        let token = login(server.addr());
        let ids = served_ids(server.addr(), &token);
        select(server.addr(), &ids[1], &token);
    }

    // The same preferences, a session serving only the first project.
    let server = server_at(prefs.path(), std::slice::from_ref(&a));
    let token = login(server.addr());

    assert_eq!(
        served_active(server.addr(), &token),
        serde_json::Value::Null,
        "a path nothing resolves names no project"
    );
}

#[test]
fn the_active_id_always_names_a_project_in_the_list_beside_it() {
    // The two are read under one catalog lock. Were they read separately, a
    // repository opening in between could yield an id the list does not carry
    // — and a client that cannot show its remembered project falls back to the
    // first tab and records that, losing the selection for good.
    let prefs = tempfile::TempDir::new().unwrap();
    let (_a, a) = make_repo();
    let (_b, b) = make_repo();
    let server = server_at(prefs.path(), &[a, b.clone()]);
    let token = login(server.addr());
    let ids = served_ids(server.addr(), &token);
    select(server.addr(), &ids[1], &token);

    // Churn the catalog while the bootstrap is being served repeatedly.
    for _ in 0..8 {
        delete(
            server.addr(),
            &format!("/api/repos?repo={}", ids[1]),
            Some(&token),
        );
        let list = get(server.addr(), "/api/repos", Some(&token));
        let value: serde_json::Value = serde_json::from_str(body_of(&list)).unwrap();
        assert_active_is_listed(&value);

        let body = serde_json::json!({ "path": b }).to_string();
        post(server.addr(), "/api/repos", &body, Some(&token));
        let list = get(server.addr(), "/api/repos", Some(&token));
        let value: serde_json::Value = serde_json::from_str(body_of(&list)).unwrap();
        assert_active_is_listed(&value);
    }
}

fn assert_active_is_listed(bootstrap: &serde_json::Value) {
    let Some(active) = bootstrap["active_repo"].as_str() else {
        return;
    };
    let listed = bootstrap["repos"]
        .as_array()
        .unwrap()
        .iter()
        .any(|repo| repo["id"] == active);
    assert!(
        listed,
        "active {active} is missing from {:?}",
        bootstrap["repos"]
    );
}

#[test]
fn selecting_a_project_that_is_not_served_is_rejected() {
    let prefs = tempfile::TempDir::new().unwrap();
    let (_a, a) = make_repo();
    let server = server_at(prefs.path(), &[a]);
    let token = login(server.addr());
    let ids = served_ids(server.addr(), &token);
    select(server.addr(), &ids[0], &token);

    let response = select(server.addr(), "r9999", &token);

    assert!(response.starts_with("HTTP/1.1 400"), "got: {response}");
    // Rejected whole: the earlier selection is still the stored one.
    assert_eq!(served_active(server.addr(), &token), ids[0].as_str());
}

#[test]
fn selecting_a_project_leaves_the_other_preferences_untouched() {
    let prefs = tempfile::TempDir::new().unwrap();
    let (_a, a) = make_repo();
    let server = server_at(prefs.path(), &[a]);
    let token = login(server.addr());
    let ids = served_ids(server.addr(), &token);
    post(server.addr(), "/api/prefs", "{\"accent\":3}", Some(&token));

    let stored = select(server.addr(), &ids[0], &token);

    let echoed: serde_json::Value = serde_json::from_str(body_of(&stored)).unwrap();
    assert_eq!(echoed["accent"], 3);
}

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
