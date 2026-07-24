use super::{
    body_of, get, log_page, seeded_server, server_with_paged_history,
};
use crate::test_util::run_git;
use crate::web::viewer::limits;

#[test]
fn the_log_reports_more_history_and_serves_the_next_page() {
    let (dir, server, token, id, _path) = server_with_paged_history();

    let first = log_page(&server, &token, &format!("repo={id}"));
    let anchor = first["head"].as_str().expect("first page carries an anchor");
    let second = log_page(
        &server,
        &token,
        &format!("repo={id}&from={anchor}&skip={}", limits::MAX_LOG_PAGE),
    );

    assert_eq!(first["commits"].as_array().unwrap().len(), limits::MAX_LOG_PAGE);
    // The page is full *and* the history continues — the distinction the
    // extra fetched entry exists to make.
    assert_eq!(first["truncated"], true);
    assert_eq!(second["commits"].as_array().unwrap().len(), 1);
    assert_eq!(second["truncated"], false);
    assert_eq!(second["commits"][0]["summary"], "c0");
    drop(dir);
}

#[test]
fn a_pinned_log_page_ignores_commits_made_after_the_first() {
    let (dir, server, token, id, path) = server_with_paged_history();
    let first = log_page(&server, &token, &format!("repo={id}"));
    let anchor = first["head"].as_str().unwrap().to_string();
    let newest = first["commits"][0]["oid"].as_str().unwrap().to_string();

    // A commit lands between the two page requests, as one made in the
    // terminal panel below the list would.
    std::fs::write(std::path::Path::new(&path).join("late"), "x").unwrap();
    run_git(&path, &["add", "."]);
    run_git(&path, &["commit", "-m", "late"]);

    let second = log_page(
        &server,
        &token,
        &format!("repo={id}&from={anchor}&skip={}", limits::MAX_LOG_PAGE),
    );

    assert_eq!(anchor, newest, "the anchor is the walk's first commit");
    // Without pinning, this page would start at c1 — repeating a commit the
    // client already holds.
    assert_eq!(second["commits"][0]["summary"], "c0");
    assert_eq!(second["commits"].as_array().unwrap().len(), 1);
    drop(dir);
}

#[test]
fn the_log_rejects_a_malformed_anchor_or_skip() {
    let (dir, server, token, id) = seeded_server();

    let bad_from = get(
        server.addr(),
        &format!("/api/log?repo={id}&from=not-an-oid"),
        Some(&token),
    );
    let bad_skip = get(
        server.addr(),
        &format!("/api/log?repo={id}&skip=-1"),
        Some(&token),
    );
    // Falling back to HEAD would answer a different question than the one
    // asked, and the client pages against what it gets back.
    assert!(bad_from.starts_with("HTTP/1.1 400"), "got: {bad_from}");
    assert!(bad_skip.starts_with("HTTP/1.1 400"), "got: {bad_skip}");
    drop(dir);
}

#[test]
fn a_skip_past_the_end_of_history_is_an_empty_last_page() {
    // Not an error and not a ceiling: the revwalk simply runs out. A skip
    // far beyond the history costs what walking the history costs, which is
    // why the parameter needs no cap.
    let (dir, server, token, id) = seeded_server();

    let value = log_page(&server, &token, &format!("repo={id}&skip=100000000"));

    assert_eq!(value["commits"].as_array().unwrap().len(), 0);
    assert_eq!(value["truncated"], false);
    drop(dir);
}

#[test]
fn tree_lists_a_directory_level() {
    let (dir, server, token, id) = seeded_server();

    let response = get(server.addr(), &format!("/api/tree?repo={id}"), Some(&token));
    let value: serde_json::Value = serde_json::from_str(body_of(&response)).unwrap();

    let names: Vec<_> = value["entries"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["name"].as_str().unwrap())
        .collect();
    assert!(names.contains(&"src"), "got: {names:?}");
    assert!(!names.contains(&".git"), "git metadata must not be listed");
    drop(dir);
}

#[test]
fn tree_search_finds_a_nested_file_by_name() {
    let (dir, server, token, id) = seeded_server();

    let response = get(
        server.addr(),
        &format!("/api/tree/search?repo={id}&q=main"),
        Some(&token),
    );
    let value: serde_json::Value = serde_json::from_str(body_of(&response)).unwrap();

    let paths: Vec<_> = value["matches"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| m["path"].as_str().unwrap())
        .collect();
    // The match lives one level down, which the single-level /api/tree could
    // not surface.
    assert_eq!(paths, vec!["src/main.rs"]);
    assert_eq!(value["truncated"], false);
    drop(dir);
}

#[test]
fn tree_search_with_an_empty_query_returns_no_matches() {
    let (dir, server, token, id) = seeded_server();

    let response = get(
        server.addr(),
        &format!("/api/tree/search?repo={id}&q="),
        Some(&token),
    );
    let value: serde_json::Value = serde_json::from_str(body_of(&response)).unwrap();

    assert!(value["matches"].as_array().unwrap().is_empty());
    drop(dir);
}

#[test]
fn a_traversal_path_is_refused_by_every_route_that_takes_one() {
    let (dir, server, token, id) = seeded_server();

    for route in ["tree", "file", "diff"] {
        for attack in ["../../etc/passwd", ".git/config", "src/../.git/config"] {
            let encoded = attack.replace('/', "%2F");
            let response = get(
                server.addr(),
                &format!("/api/{route}?repo={id}&path={encoded}"),
                Some(&token),
            );
            assert!(
                response.starts_with("HTTP/1.1 400"),
                "{route} accepted {attack:?}: {response}"
            );
        }
    }
    drop(dir);
}

#[test]
fn an_error_response_leaks_no_filesystem_detail() {
    let (dir, server, token, id) = seeded_server();

    let response = get(
        server.addr(),
        &format!("/api/file?repo={id}&path=nope.txt"),
        Some(&token),
    );

    let body = body_of(&response);
    assert!(!body.contains('/'), "a path leaked into the error: {body}");
    assert!(
        !body.contains("No such file"),
        "the io error leaked: {body}"
    );
    drop(dir);
}

#[test]
fn file_returns_worktree_content() {
    let (dir, server, token, id) = seeded_server();

    let response = get(
        server.addr(),
        &format!("/api/file?repo={id}&path=src%2Fmain.rs"),
        Some(&token),
    );
    let value: serde_json::Value = serde_json::from_str(body_of(&response)).unwrap();

    // Content is returned as per-line, syntax-highlighted spans. Rebuild the
    // text from them and confirm it round-trips.
    let text: String = value["lines"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|line| line.as_array().unwrap())
        .map(|span| span["t"].as_str().unwrap())
        .collect::<Vec<_>>()
        .join("");
    assert_eq!(text, "fn main() {}");
    assert_eq!(value["truncated"], false);
    drop(dir);
}

#[test]
fn log_returns_commits() {
    let (dir, server, token, id) = seeded_server();

    let response = get(server.addr(), &format!("/api/log?repo={id}"), Some(&token));
    let value: serde_json::Value = serde_json::from_str(body_of(&response)).unwrap();

    let commits = value["commits"].as_array().unwrap();
    assert_eq!(commits.len(), 1);
    assert_eq!(commits[0]["summary"], "init");
    assert_eq!(
        commits[0]["oid"].as_str().unwrap().len(),
        40,
        "the oid must be hex, not libgit2's own shape"
    );
    drop(dir);
}