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

    let response = post(server.addr(), "/api/prefs", "{\"accent\":\"red\"}", Some(&token));

    assert!(response.starts_with("HTTP/1.1 400"), "got: {response}");
    let list = get(server.addr(), "/api/repos", Some(&token));
    let value: serde_json::Value = serde_json::from_str(body_of(&list)).unwrap();
    assert_eq!(value["accent"], 0);
}