//! `/api/log/decorations`: the refs and upstream divergence for the whole
//! repository, apart from the log pages they decorate.

use super::{body_of, get, seeded_server};
use crate::test_util::run_git;

#[test]
fn decorations_name_the_branch_head_points_at_and_its_tags() {
    let (dir, server, token, id) = seeded_server();
    let repo_path = server
        .state
        .session
        .catalog()
        .get(&id)
        .unwrap()
        .path
        .clone();
    run_git(&repo_path, &["tag", "v1"]);
    let log = get(server.addr(), &format!("/api/log?repo={id}"), Some(&token));
    let log: serde_json::Value = serde_json::from_str(body_of(&log)).unwrap();
    let tip = log["commits"][0]["oid"].as_str().unwrap().to_string();

    let response = get(
        server.addr(),
        &format!("/api/log/decorations?repo={id}"),
        Some(&token),
    );
    let value: serde_json::Value = serde_json::from_str(body_of(&response)).unwrap();

    assert!(response.starts_with("HTTP/1.1 200"), "got: {response}");
    let chips = value["refs"][&tip]
        .as_array()
        .expect("the tip is decorated");
    assert_eq!(chips[0]["kind"], "head", "HEAD comes first: {chips:?}");
    assert!(
        chips
            .iter()
            .any(|c| c["kind"] == "tag" && c["name"] == "v1")
    );
    // No upstream in the fixture repository: nothing to diverge from.
    assert_eq!(value["ahead"], serde_json::json!([]));
    assert_eq!(value["behind"], serde_json::json!([]));
    drop(dir);
}

#[test]
fn log_pages_no_longer_carry_refs() {
    // They would go stale on every loaded row when a ref moved.
    let (dir, server, token, id) = seeded_server();
    let log = get(server.addr(), &format!("/api/log?repo={id}"), Some(&token));
    let log: serde_json::Value = serde_json::from_str(body_of(&log)).unwrap();

    assert!(log["commits"][0].get("refs").is_none());
    assert!(log["commits"][0].get("divergence").is_none());
    drop(dir);
}

#[test]
fn decorations_require_a_known_repository() {
    let (dir, server, token, _id) = seeded_server();
    let response = get(
        server.addr(),
        "/api/log/decorations?repo=nope",
        Some(&token),
    );
    assert!(!response.starts_with("HTTP/1.1 200"), "got: {response}");
    drop(dir);
}
