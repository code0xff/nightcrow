//! What a project was last showing, over the wire.

use super::*;

fn view_server() -> (
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
fn what_a_project_was_showing_is_served_to_later_clients() {
    let (_prefs_dir, _repo_dir, server, token, id) = view_server();
    let body = serde_json::json!({
        "view": {
            "repo": id,
            "tab": "tree",
            "file": { "path": "src/main.rs", "face": "source" },
            "tree_expanded": ["src"],
        }
    });

    let stored = post(server.addr(), "/api/prefs", &body.to_string(), Some(&token));
    let echoed: serde_json::Value = serde_json::from_str(body_of(&stored)).unwrap();
    assert_eq!(echoed["last_view"][&id]["tab"], "tree");
    assert_eq!(echoed["last_view"][&id]["file"]["face"], "source");

    // The bootstrap is what a page opening later reads, and it is the reason
    // this is stored at all.
    let again = get(server.addr(), "/api/repos", Some(&token));
    let value: serde_json::Value = serde_json::from_str(body_of(&again)).unwrap();
    let view = &value["last_view"][&id];
    assert_eq!(view["file"]["path"], "src/main.rs");
    assert_eq!(view["tree_expanded"], serde_json::json!(["src"]));
}

#[test]
fn a_view_naming_something_this_build_does_not_know_is_refused() {
    let (_prefs_dir, _repo_dir, server, token, id) = view_server();

    for body in [
        serde_json::json!({ "view": { "repo": id, "tab": "diff" } }),
        serde_json::json!({ "view": { "repo": id, "tab": "status",
            "file": { "path": "src/main.rs", "face": "rendered" } } }),
        serde_json::json!({ "view": { "repo": "r9999", "tab": "status" } }),
    ] {
        let refused = post(server.addr(), "/api/prefs", &body.to_string(), Some(&token));
        assert!(refused.starts_with("HTTP/1.1 400"), "got: {refused}");
    }

    let again = get(server.addr(), "/api/repos", Some(&token));
    let value: serde_json::Value = serde_json::from_str(body_of(&again)).unwrap();
    assert_eq!(value["last_view"], serde_json::json!({}));
}

/// A path is not answered with a 400 — it is dropped where it would be stored,
/// which is the same door the prefs file comes through (`prefs::repo_view`).
/// What must not happen is the view being stored with it.
#[test]
fn a_path_leaving_the_project_does_not_reach_the_file() {
    let (_prefs_dir, _repo_dir, server, token, id) = view_server();
    let body = serde_json::json!({
        "view": {
            "repo": id,
            "tab": "tree",
            "file": { "path": "../../etc/passwd", "face": "source" },
            "tree_expanded": ["../elsewhere"],
        }
    });

    post(server.addr(), "/api/prefs", &body.to_string(), Some(&token));

    let again = get(server.addr(), "/api/repos", Some(&token));
    let value: serde_json::Value = serde_json::from_str(body_of(&again)).unwrap();
    let view = &value["last_view"][&id];
    assert_eq!(view["tab"], "tree", "the tab is still worth restoring");
    assert!(view["file"].is_null(), "got: {view}");
    assert_eq!(view["tree_expanded"], serde_json::json!([]));
}
