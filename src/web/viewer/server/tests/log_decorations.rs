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

#[test]
fn a_capped_answer_keeps_head_and_drops_tags_first() {
    // Many tags on one repository: the cut gives up the tags, says it did, and
    // never the label for where you are.
    let (dir, server, _token, id) = seeded_server();
    let repo_path = server
        .state
        .session
        .catalog()
        .get(&id)
        .unwrap()
        .path
        .clone();
    for n in 0..5 {
        run_git(&repo_path, &["tag", &format!("v{n}")]);
    }
    let repo = git2::Repository::open(&repo_path).unwrap();
    let decorations = crate::git::diff::load_log_decorations(&repo).unwrap();

    let dto = crate::web::viewer::dto::LogDecorationsDto::capped(&decorations, 3);

    assert!(dto.truncated);
    let kinds: Vec<&str> = dto.refs.values().flatten().map(|r| r.kind).collect();
    assert_eq!(kinds.len(), 3);
    assert_eq!(kinds[0], "head", "got: {kinds:?}");
    drop(dir);
}
