use super::*;

fn arrangement_server() -> (
    tempfile::TempDir,
    tempfile::TempDir,
    ViewerServer,
    String,
    String,
) {
    let prefs_dir = tempfile::TempDir::new().unwrap();
    let (repo_dir, repo) = crate::test_util::make_repo();
    let server = server_with(
        std::slice::from_ref(&repo),
        crate::config::AgentIndicatorConfig::default(),
        Some(prefs_dir.path()),
    );
    let token = login(server.addr());
    let list = get(server.addr(), "/api/repos", Some(&token));
    let value: serde_json::Value = serde_json::from_str(body_of(&list)).unwrap();
    let id = value["repos"][0]["id"].as_str().unwrap().to_string();
    (prefs_dir, repo_dir, server, token, id)
}

#[test]
fn a_projects_arrangement_is_served_to_later_clients() {
    let (_prefs_dir, _repo_dir, server, token, id) = arrangement_server();
    let body = serde_json::json!({ "maximized": { "repo": id, "panel": "terminal" } });

    let stored = post(server.addr(), "/api/prefs", &body.to_string(), Some(&token));
    let echoed: serde_json::Value = serde_json::from_str(body_of(&stored)).unwrap();
    assert_eq!(echoed["maximized"][&id], "terminal");

    let again = get(server.addr(), "/api/repos", Some(&token));
    let value: serde_json::Value = serde_json::from_str(body_of(&again)).unwrap();
    assert_eq!(value["maximized"][&id], "terminal");

    let body = serde_json::json!({ "maximized": { "repo": id, "panel": null } });
    let stored = post(server.addr(), "/api/prefs", &body.to_string(), Some(&token));
    let echoed: serde_json::Value = serde_json::from_str(body_of(&stored)).unwrap();
    assert_eq!(echoed["maximized"], serde_json::json!({}));
}

#[test]
fn an_arrangement_naming_something_unrenderable_is_refused() {
    let (_prefs_dir, _repo_dir, server, token, id) = arrangement_server();

    for body in [
        serde_json::json!({ "maximized": { "repo": id, "panel": "diff" } }),
        serde_json::json!({ "maximized": { "repo": "r9999", "panel": "files" } }),
    ] {
        let refused = post(server.addr(), "/api/prefs", &body.to_string(), Some(&token));
        assert!(refused.starts_with("HTTP/1.1 400"), "got: {refused}");
    }

    let again = get(server.addr(), "/api/repos", Some(&token));
    let value: serde_json::Value = serde_json::from_str(body_of(&again)).unwrap();
    assert_eq!(value["maximized"], serde_json::json!({}));
}
