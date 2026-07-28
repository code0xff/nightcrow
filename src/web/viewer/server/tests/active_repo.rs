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
    assert_eq!(served_active(server.addr(), &token), serde_json::Value::Null);
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
    let prefs = tempfile::TempDir::new().unwrap();
    let (_a, a) = make_repo();
    let (_b, b) = make_repo();
    let server = server_at(prefs.path(), &[a, b]);
    let token = login(server.addr());
    let ids = served_ids(server.addr(), &token);
    select(server.addr(), &ids[1], &token);

    let closed = delete(
        server.addr(),
        &format!("/api/repos?repo={}", ids[1]),
        Some(&token),
    );
    assert!(closed.starts_with("HTTP/1.1 200"), "got: {closed}");

    // A closed project has no id to name. The path stays on file rather than
    // being cleared, but nothing resolves it, so the client falls back.
    assert_eq!(served_active(server.addr(), &token), serde_json::Value::Null);
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
