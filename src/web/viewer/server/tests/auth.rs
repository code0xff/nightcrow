use super::{VIEWER_SESSION_COOKIE, body_of, get, login, post, request, seeded_server, server};
use crate::test_util::make_repo;

#[test]
fn api_requires_authentication() {
    let (dir, path) = make_repo();
    let server = server(&[path]);

    let response = get(server.addr(), "/api/repos", None);

    assert!(response.starts_with("HTTP/1.1 401"), "got: {response}");
    drop(dir);
}

#[test]
fn opening_a_repository_adds_it_to_the_served_set() {
    // Start empty, the way `serve` with no --repo now does, then open a
    // repository from the browser.
    let server = server(&[]);
    let token = login(server.addr());
    let (dir, path) = make_repo();
    let body = format!("{{\"path\":{}}}", serde_json::to_string(&path).unwrap());

    let opened = post(server.addr(), "/api/repos", &body, Some(&token));
    assert!(opened.starts_with("HTTP/1.1 200"), "got: {opened}");

    let list = get(server.addr(), "/api/repos", Some(&token));
    let value: serde_json::Value = serde_json::from_str(body_of(&list)).unwrap();
    assert_eq!(
        value["repos"].as_array().unwrap().len(),
        1,
        "the opened repository must appear in the served set"
    );
    drop(dir);
}

#[test]
fn opening_a_repository_requires_authentication() {
    let server = server(&[]);
    let (dir, path) = make_repo();
    let body = format!("{{\"path\":{}}}", serde_json::to_string(&path).unwrap());

    let response = post(server.addr(), "/api/repos", &body, None);

    assert!(response.starts_with("HTTP/1.1 401"), "got: {response}");
    drop(dir);
}

#[test]
fn browse_lists_subdirectories_and_flags_repos() {
    let root = tempfile::tempdir().unwrap();
    std::fs::create_dir(root.path().join("alpha")).unwrap();
    std::fs::create_dir_all(root.path().join("beta").join(".git")).unwrap();
    std::fs::write(root.path().join("afile.txt"), b"x").unwrap();
    let server = server(&[]);
    let token = login(server.addr());

    let path = root.path().to_string_lossy();
    let response = get(
        server.addr(),
        &format!("/api/browse?path={path}"),
        Some(&token),
    );
    assert!(response.starts_with("HTTP/1.1 200"), "got: {response}");

    let value: serde_json::Value = serde_json::from_str(body_of(&response)).unwrap();
    let list = value["entries"].as_array().unwrap();
    let names: Vec<&str> = list.iter().map(|e| e["name"].as_str().unwrap()).collect();
    assert!(
        names.contains(&"alpha") && names.contains(&"beta"),
        "expected sub-directories, got: {names:?}"
    );
    assert!(!names.contains(&"afile.txt"), "files must be excluded");
    let beta = list.iter().find(|e| e["name"] == "beta").unwrap();
    assert_eq!(beta["is_repo"], true, "a .git folder marks a repo");
}

#[test]
fn closing_a_repository_removes_it_from_the_served_set() {
    let (dir, path) = make_repo();
    let server = server(&[path]);
    let token = login(server.addr());

    let list = get(server.addr(), "/api/repos", Some(&token));
    let value: serde_json::Value = serde_json::from_str(body_of(&list)).unwrap();
    let id = value["repos"][0]["id"].as_str().unwrap().to_string();

    let closed = super::delete(
        server.addr(),
        &format!("/api/repos?repo={id}"),
        Some(&token),
    );
    assert!(closed.starts_with("HTTP/1.1 200"), "got: {closed}");

    let after = get(server.addr(), "/api/repos", Some(&token));
    let value: serde_json::Value = serde_json::from_str(body_of(&after)).unwrap();
    assert_eq!(
        value["repos"].as_array().unwrap().len(),
        0,
        "the closed repository must be gone from the served set"
    );
    drop(dir);
}

#[test]
fn closing_an_unknown_repository_is_a_404() {
    let (dir, path) = make_repo();
    let server = server(&[path]);
    let token = login(server.addr());

    let response = super::delete(server.addr(), "/api/repos?repo=nope", Some(&token));

    assert!(response.starts_with("HTTP/1.1 404"), "got: {response}");
    drop(dir);
}

#[test]
fn opening_a_nonexistent_path_is_rejected() {
    let server = server(&[]);
    let token = login(server.addr());

    let response = post(
        server.addr(),
        "/api/repos",
        "{\"path\":\"/definitely/not/a/real/directory\"}",
        Some(&token),
    );

    assert!(response.starts_with("HTTP/1.1 400"), "got: {response}");
}

#[test]
fn auth_is_checked_before_the_repository_is_looked_up() {
    // An unauthenticated request must not be able to distinguish a real id
    // from a made-up one — that would enumerate the served repositories.
    let (dir, path) = make_repo();
    let server = server(&[path]);
    let token = login(server.addr());
    let real = {
        let listing = get(server.addr(), "/api/repos", Some(&token));
        let value: serde_json::Value = serde_json::from_str(body_of(&listing)).unwrap();
        value["repos"][0]["id"].as_str().unwrap().to_string()
    };

    let known = get(server.addr(), &format!("/api/status?repo={real}"), None);
    let unknown = get(server.addr(), "/api/status?repo=r9999", None);

    assert!(known.starts_with("HTTP/1.1 401"), "got: {known}");
    assert!(unknown.starts_with("HTTP/1.1 401"), "got: {unknown}");
    drop(dir);
}

#[test]
fn a_rebound_host_is_refused_on_a_loopback_bind() {
    // DNS rebinding: the attacker controls Origin *and* Host, so they
    // agree and the origin check alone would pass. Only the Host check
    // denies the same-origin foothold.
    let (dir, path) = make_repo();
    let server = server(&[path]);
    let token = login(server.addr());

    let response = request(
        server.addr(),
        &format!(
            "GET /api/repos HTTP/1.1\r\nHost: evil.example\r\n\
             Origin: http://evil.example\r\n\
             Cookie: {VIEWER_SESSION_COOKIE}={token}\r\nConnection: close\r\n\r\n"
        ),
    );

    assert!(response.starts_with("HTTP/1.1 403"), "got: {response}");
    drop(dir);
}

#[test]
fn logout_revokes_the_session_server_side() {
    // Clearing the cookie is not enough: cookies are not port-isolated, so
    // another loopback service is same-site and may already hold the token.
    let (dir, path) = make_repo();
    let server = server(&[path]);
    let token = login(server.addr());
    assert!(get(server.addr(), "/api/repos", Some(&token)).starts_with("HTTP/1.1 200"));

    get(server.addr(), "/logout", Some(&token));

    let after = get(server.addr(), "/api/repos", Some(&token));
    assert!(
        after.starts_with("HTTP/1.1 401"),
        "the token must stop working immediately: {after}"
    );
    drop(dir);
}

#[test]
fn a_cross_origin_request_is_refused_before_auth() {
    let (dir, path) = make_repo();
    let server = server(&[path]);
    let token = login(server.addr());

    let response = request(
        server.addr(),
        &format!(
            "GET /api/repos HTTP/1.1\r\nHost: 127.0.0.1\r\nOrigin: http://evil.example\r\n\
             Cookie: {VIEWER_SESSION_COOKIE}={token}\r\nConnection: close\r\n\r\n"
        ),
    );

    assert!(response.starts_with("HTTP/1.1 403"), "got: {response}");
    drop(dir);
}

#[test]
fn an_unknown_repo_id_is_a_404_for_an_authenticated_client() {
    let (dir, path) = make_repo();
    let server = server(&[path]);
    let token = login(server.addr());

    let response = get(server.addr(), "/api/status?repo=r9999", Some(&token));

    assert!(response.starts_with("HTTP/1.1 404"), "got: {response}");
    drop(dir);
}

#[test]
fn a_missing_repo_parameter_is_a_400() {
    let (dir, path) = make_repo();
    let server = server(&[path]);
    let token = login(server.addr());

    let response = get(server.addr(), "/api/status", Some(&token));

    assert!(response.starts_with("HTTP/1.1 400"), "got: {response}");
    drop(dir);
}

#[test]
fn a_non_get_method_is_rejected() {
    let (dir, server, token, id) = seeded_server();

    let response = request(
        server.addr(),
        &format!(
            "DELETE /api/status?repo={id} HTTP/1.1\r\nHost: 127.0.0.1\r\n\
             Cookie: {VIEWER_SESSION_COOKIE}={token}\r\nConnection: close\r\n\r\n"
        ),
    );

    assert!(response.starts_with("HTTP/1.1 405"), "got: {response}");
    drop(dir);
}
