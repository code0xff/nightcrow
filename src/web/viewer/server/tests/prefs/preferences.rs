use super::*;

#[test]
fn a_stored_accent_is_served_to_later_clients() {
    let (_dir, server, token) = prefs_server();

    let stored = post(server.addr(), "/api/prefs", "{\"accent\":3}", Some(&token));
    assert!(stored.starts_with("HTTP/1.1 200"), "got: {stored}");

    let list = get(server.addr(), "/api/repos", Some(&token));
    let value: serde_json::Value = serde_json::from_str(body_of(&list)).unwrap();
    assert_eq!(value["accent"], 3);
}

#[test]
fn layout_bounds_are_clamped_and_persisted() {
    let (_dir, server, token) = prefs_server();
    let stored = post(
        server.addr(),
        "/api/prefs",
        "{\"upper_pct\":99}",
        Some(&token),
    );
    let echoed: serde_json::Value = serde_json::from_str(body_of(&stored)).unwrap();
    assert_eq!(echoed["upper_pct"], crate::session::prefs::MAX_UPPER_PCT);

    let list = get(server.addr(), "/api/repos", Some(&token));
    let value: serde_json::Value = serde_json::from_str(body_of(&list)).unwrap();
    assert_eq!(value["upper_pct"], crate::session::prefs::MAX_UPPER_PCT);
}

#[test]
fn setting_one_preference_leaves_the_other_untouched() {
    let (_dir, server, token) = prefs_server();

    post(server.addr(), "/api/prefs", "{\"accent\":3}", Some(&token));
    let stored = post(
        server.addr(),
        "/api/prefs",
        "{\"upper_pct\":70}",
        Some(&token),
    );

    let echoed: serde_json::Value = serde_json::from_str(body_of(&stored)).unwrap();
    assert_eq!(echoed["accent"], 3);
    assert_eq!(echoed["upper_pct"], 70);
}

#[test]
fn the_sidebar_width_is_no_longer_a_preference_the_server_knows() {
    // It moved to the browser's own storage. A body naming only it is a body
    // naming nothing this server stores, and is answered as such rather than
    // accepted and dropped — silence here would let a stale page believe its
    // width was being shared.
    let (_dir, server, token) = prefs_server();

    let stored = post(
        server.addr(),
        "/api/prefs",
        "{\"sidebar_width\":500}",
        Some(&token),
    );

    assert!(stored.starts_with("HTTP/1.1 400"), "got: {stored}");
    let list = get(server.addr(), "/api/repos", Some(&token));
    let value: serde_json::Value = serde_json::from_str(body_of(&list)).unwrap();
    assert!(value.get("sidebar_width").is_none());
}

#[test]
fn invalid_preference_bodies_are_rejected_without_mutation() {
    let (_dir, server, token) = prefs_server();

    for body in ["{\"nope\":1}", "{\"accent\":\"red\"}"] {
        let response = post(server.addr(), "/api/prefs", body, Some(&token));
        assert!(
            response.starts_with("HTTP/1.1 400"),
            "body {body}: {response}"
        );
    }

    let list = get(server.addr(), "/api/repos", Some(&token));
    let value: serde_json::Value = serde_json::from_str(body_of(&list)).unwrap();
    assert_eq!(value["accent"], 0);
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
