//! What `GET /api/repos` answers besides the repositories themselves.
//!
//! The one response every client already polls carries the session-wide facts
//! each of them has to agree on, so this is where each of those is pinned.

use super::{body_of, get, login, server_with};

#[test]
fn the_repository_list_serves_the_configured_recently_touched_settings() {
    // The client fades its file list on this window; reading the config
    // from the server is what keeps it on the TUI's window rather than a
    // second default that drifts.
    let server = server_with(
        &[],
        crate::config::AgentIndicatorConfig {
            enabled: false,
            hot_window_secs: 42,
            auto_follow: true,
        },
        None,
    );
    let token = login(server.addr());

    let response = get(server.addr(), "/api/repos", Some(&token));
    let value: serde_json::Value = serde_json::from_str(body_of(&response)).unwrap();

    assert_eq!(value["hot"]["enabled"], false);
    assert_eq!(value["hot"]["window_secs"], 42);
    // `auto_follow` moves a TUI selection; the browser has no analogue and
    // must not be told about it.
    assert!(value["hot"].get("auto_follow").is_none());
}

#[test]
fn the_repository_list_serves_the_server_clock_for_dating_mtimes() {
    // `mtime` is an absolute instant on this machine's clock, so a browser
    // on a device whose clock disagrees needs the reference to subtract.
    use crate::web::viewer::dto::server_now_millis;

    let server = server_with(&[], crate::config::AgentIndicatorConfig::default(), None);
    let token = login(server.addr());
    let before = server_now_millis();

    let response = get(server.addr(), "/api/repos", Some(&token));
    let value: serde_json::Value = serde_json::from_str(body_of(&response)).unwrap();

    let now_ms = value["now_ms"].as_u64().expect("now_ms is a number");
    let after = server_now_millis();
    assert!(
        (before..=after).contains(&now_ms),
        "now_ms {now_ms} outside the request window {before}..={after}",
    );
}

#[test]
fn the_repository_list_names_the_frontend_build_it_was_served_with() {
    // The client holds the first one it sees and watches for it to change,
    // which is how a tab learns the server was replaced under it. Carried by
    // the poll it already makes rather than by an endpoint of its own.
    let server = server_with(&[], crate::config::AgentIndicatorConfig::default(), None);
    let token = login(server.addr());

    let response = get(server.addr(), "/api/repos", Some(&token));
    let value: serde_json::Value = serde_json::from_str(body_of(&response)).unwrap();

    assert_eq!(
        value["viewer_build"].as_str(),
        crate::web::viewer::assets::build_id().as_deref(),
        "the response must name the build this server serves"
    );
}
