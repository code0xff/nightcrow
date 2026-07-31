use super::{body_of, get, login, post, server_with};

#[test]
fn a_stored_accent_is_served_to_every_later_client() {
    // The point of storing it server-side: a second device (a second
    // request, here) sees the choice without having made it.
    let dir = tempfile::TempDir::new().unwrap();
    let server = server_with(
        &[],
        crate::config::AgentIndicatorConfig::default(),
        Some(dir.path()),
    );
    let token = login(server.addr());

    let stored = post(server.addr(), "/api/prefs", "{\"accent\":3}", Some(&token));
    assert!(stored.starts_with("HTTP/1.1 200"), "got: {stored}");

    let list = get(server.addr(), "/api/repos", Some(&token));
    let value: serde_json::Value = serde_json::from_str(body_of(&list)).unwrap();
    assert_eq!(value["accent"], 3);
}

#[test]
fn a_stored_sidebar_width_is_clamped_and_served_to_every_later_client() {
    let dir = tempfile::TempDir::new().unwrap();
    let server = server_with(
        &[],
        crate::config::AgentIndicatorConfig::default(),
        Some(dir.path()),
    );
    let token = login(server.addr());

    // Past the ceiling: the write echoes the clamped value, and a later
    // client's bootstrap carries the same clamped width — not the raw ask.
    let stored = post(
        server.addr(),
        "/api/prefs",
        "{\"sidebar_width\":5000}",
        Some(&token),
    );
    let echoed: serde_json::Value = serde_json::from_str(body_of(&stored)).unwrap();
    assert_eq!(
        echoed["sidebar_width"],
        crate::web::viewer::prefs::MAX_SIDEBAR_WIDTH
    );

    let list = get(server.addr(), "/api/repos", Some(&token));
    let value: serde_json::Value = serde_json::from_str(body_of(&list)).unwrap();
    assert_eq!(
        value["sidebar_width"],
        crate::web::viewer::prefs::MAX_SIDEBAR_WIDTH
    );
}

#[test]
fn a_stored_upper_pct_is_clamped_and_served_to_every_later_client() {
    let dir = tempfile::TempDir::new().unwrap();
    let server = server_with(
        &[],
        crate::config::AgentIndicatorConfig::default(),
        Some(dir.path()),
    );
    let token = login(server.addr());

    // Past the ceiling: the write echoes the clamped percentage, and a later
    // client's bootstrap opens at the same split rather than the raw ask.
    let stored = post(
        server.addr(),
        "/api/prefs",
        "{\"upper_pct\":99}",
        Some(&token),
    );
    let echoed: serde_json::Value = serde_json::from_str(body_of(&stored)).unwrap();
    assert_eq!(
        echoed["upper_pct"],
        crate::web::viewer::prefs::MAX_UPPER_PCT
    );

    let list = get(server.addr(), "/api/repos", Some(&token));
    let value: serde_json::Value = serde_json::from_str(body_of(&list)).unwrap();
    assert_eq!(value["upper_pct"], crate::web::viewer::prefs::MAX_UPPER_PCT);
}

#[test]
fn mkdir_creates_a_folder_inside_the_browsed_directory() {
    let dir = tempfile::TempDir::new().unwrap();
    let server = server_with(
        &[],
        crate::config::AgentIndicatorConfig::default(),
        Some(dir.path()),
    );
    let token = login(server.addr());

    let body = serde_json::json!({
        "path": dir.path().to_str().unwrap(),
        "name": "scratch",
    })
    .to_string();
    let created = post(server.addr(), "/api/mkdir", &body, Some(&token));
    assert!(created.starts_with("HTTP/1.1 200"), "got: {created}");
    assert!(
        dir.path().join("scratch").is_dir(),
        "the folder must exist on disk"
    );

    let value: serde_json::Value = serde_json::from_str(body_of(&created)).unwrap();
    let path = value["path"].as_str().unwrap();
    assert!(
        std::path::Path::new(path).ends_with("scratch"),
        "the response names the new folder, got: {path}"
    );
}

#[test]
fn mkdir_rejects_a_name_that_would_escape_the_browsed_directory() {
    let dir = tempfile::TempDir::new().unwrap();
    let server = server_with(
        &[],
        crate::config::AgentIndicatorConfig::default(),
        Some(dir.path()),
    );
    let token = login(server.addr());

    // Separators, traversal, and a leading dot (which also covers `.git`
    // and hidden entries) are all refused so the create cannot leave the
    // browsed directory.
    for name in ["../escape", "a/b", "..", ".git", ".hidden"] {
        let body = serde_json::json!({
            "path": dir.path().to_str().unwrap(),
            "name": name,
        })
        .to_string();
        let response = post(server.addr(), "/api/mkdir", &body, Some(&token));
        assert!(
            response.starts_with("HTTP/1.1 400"),
            "name {name:?} must be rejected, got: {response}"
        );
    }
    assert!(
        !dir.path().parent().unwrap().join("escape").exists(),
        "traversal must not create anything above the parent"
    );
}

#[test]
fn mkdir_requires_authentication() {
    let dir = tempfile::TempDir::new().unwrap();
    let server = server_with(
        &[],
        crate::config::AgentIndicatorConfig::default(),
        Some(dir.path()),
    );

    let body = serde_json::json!({
        "path": dir.path().to_str().unwrap(),
        "name": "nope",
    })
    .to_string();
    let response = post(server.addr(), "/api/mkdir", &body, None);
    assert!(
        response.starts_with("HTTP/1.1 401"),
        "an unauthenticated mkdir must be refused, got: {response}"
    );
    assert!(
        !dir.path().join("nope").exists(),
        "nothing may be created without a session"
    );
}

#[test]
fn setting_one_preference_leaves_the_other_untouched() {
    let dir = tempfile::TempDir::new().unwrap();
    let server = server_with(
        &[],
        crate::config::AgentIndicatorConfig::default(),
        Some(dir.path()),
    );
    let token = login(server.addr());

    post(server.addr(), "/api/prefs", "{\"accent\":3}", Some(&token));
    let stored = post(
        server.addr(),
        "/api/prefs",
        "{\"sidebar_width\":500}",
        Some(&token),
    );

    // The width write must not reset the accent stored a moment earlier.
    let echoed: serde_json::Value = serde_json::from_str(body_of(&stored)).unwrap();
    assert_eq!(echoed["accent"], 3);
    assert_eq!(echoed["sidebar_width"], 500);
}

#[test]
fn a_preference_body_naming_nothing_known_is_rejected() {
    let dir = tempfile::TempDir::new().unwrap();
    let server = server_with(
        &[],
        crate::config::AgentIndicatorConfig::default(),
        Some(dir.path()),
    );
    let token = login(server.addr());

    let response = post(server.addr(), "/api/prefs", "{\"nope\":1}", Some(&token));

    assert!(response.starts_with("HTTP/1.1 400"), "got: {response}");
}

#[test]
fn storing_a_preference_requires_authentication() {
    let dir = tempfile::TempDir::new().unwrap();
    let server = server_with(
        &[],
        crate::config::AgentIndicatorConfig::default(),
        Some(dir.path()),
    );

    let response = post(server.addr(), "/api/prefs", "{\"accent\":3}", None);

    assert!(response.starts_with("HTTP/1.1 401"), "got: {response}");
}

#[test]
fn a_malformed_preference_body_is_rejected_without_changing_anything() {
    let dir = tempfile::TempDir::new().unwrap();
    let server = server_with(
        &[],
        crate::config::AgentIndicatorConfig::default(),
        Some(dir.path()),
    );
    let token = login(server.addr());

    let response = post(
        server.addr(),
        "/api/prefs",
        "{\"accent\":\"red\"}",
        Some(&token),
    );

    assert!(response.starts_with("HTTP/1.1 400"), "got: {response}");
    let list = get(server.addr(), "/api/repos", Some(&token));
    let value: serde_json::Value = serde_json::from_str(body_of(&list)).unwrap();
    assert_eq!(value["accent"], 0);
}

/// The point of the field: the arrangement outlives the page that set it.
#[test]
fn a_projects_arrangement_is_served_to_every_later_client() {
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
    assert_eq!(value["maximized"], serde_json::json!({}), "nothing yet");

    let body = serde_json::json!({ "maximized": { "repo": id, "panel": "terminal" } });
    let stored = post(server.addr(), "/api/prefs", &body.to_string(), Some(&token));
    let echoed: serde_json::Value = serde_json::from_str(body_of(&stored)).unwrap();
    assert_eq!(echoed["maximized"][&id], "terminal");

    // A later client — a refresh, or another device — opens arranged the same.
    let again = get(server.addr(), "/api/repos", Some(&token));
    let value: serde_json::Value = serde_json::from_str(body_of(&again)).unwrap();
    assert_eq!(value["maximized"][&id], "terminal");

    // And un-maximizing takes it back off, rather than storing a "none".
    let body = serde_json::json!({ "maximized": { "repo": id, "panel": null } });
    let stored = post(server.addr(), "/api/prefs", &body.to_string(), Some(&token));
    let echoed: serde_json::Value = serde_json::from_str(body_of(&stored)).unwrap();
    assert_eq!(echoed["maximized"], serde_json::json!({}));
    drop((repo_dir, prefs_dir));
}

#[test]
fn an_arrangement_naming_something_the_server_cannot_render_is_refused() {
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

    // A panel that is not one of the two. Refused rather than stored, or a
    // later client would be handed an arrangement it cannot apply.
    let body = serde_json::json!({ "maximized": { "repo": id, "panel": "diff" } });
    let refused = post(server.addr(), "/api/prefs", &body.to_string(), Some(&token));
    assert!(refused.starts_with("HTTP/1.1 400"), "got: {refused}");

    // A repository this session is not serving, the same way `active_repo` is.
    let body = serde_json::json!({ "maximized": { "repo": "r9999", "panel": "files" } });
    let refused = post(server.addr(), "/api/prefs", &body.to_string(), Some(&token));
    assert!(refused.starts_with("HTTP/1.1 400"), "got: {refused}");

    let again = get(server.addr(), "/api/repos", Some(&token));
    let value: serde_json::Value = serde_json::from_str(body_of(&again)).unwrap();
    assert_eq!(value["maximized"], serde_json::json!({}), "nothing stored");
    drop((repo_dir, prefs_dir));
}
