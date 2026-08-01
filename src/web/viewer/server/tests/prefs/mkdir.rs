use super::*;

#[test]
fn mkdir_creates_a_folder_inside_the_browsed_directory() {
    let (dir, server, token) = prefs_server();
    let body = serde_json::json!({
        "path": dir.path().to_str().unwrap(),
        "name": "scratch",
    })
    .to_string();

    let created = post(server.addr(), "/api/mkdir", &body, Some(&token));

    assert!(created.starts_with("HTTP/1.1 200"), "got: {created}");
    assert!(dir.path().join("scratch").is_dir());
    let value: serde_json::Value = serde_json::from_str(body_of(&created)).unwrap();
    assert!(std::path::Path::new(value["path"].as_str().unwrap()).ends_with("scratch"));
}

#[test]
fn mkdir_rejects_names_that_escape_the_browsed_directory() {
    let (dir, server, token) = prefs_server();

    for name in ["../escape", "a/b", "..", ".git", ".hidden"] {
        let body = serde_json::json!({
            "path": dir.path().to_str().unwrap(),
            "name": name,
        })
        .to_string();
        let response = post(server.addr(), "/api/mkdir", &body, Some(&token));
        assert!(response.starts_with("HTTP/1.1 400"), "{name:?}: {response}");
    }
    assert!(!dir.path().parent().unwrap().join("escape").exists());
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

    assert!(response.starts_with("HTTP/1.1 401"), "got: {response}");
    assert!(!dir.path().join("nope").exists());
}
