use super::{body_of, get, seeded_server};
use crate::test_util::run_git;

#[test]
fn commit_files_returns_the_selected_commits_changed_paths() {
    let (dir, server, token, id) = seeded_server();
    let log = get(server.addr(), &format!("/api/log?repo={id}"), Some(&token));
    let value: serde_json::Value = serde_json::from_str(body_of(&log)).unwrap();
    let oid = value["commits"][0]["oid"].as_str().unwrap();

    let response = get(
        server.addr(),
        &format!("/api/commit/files?repo={id}&oid={oid}"),
        Some(&token),
    );
    let value: serde_json::Value = serde_json::from_str(body_of(&response)).unwrap();

    assert!(response.starts_with("HTTP/1.1 200"), "got: {response}");
    assert_eq!(value["files"][0]["path"], "src/main.rs");
    assert_eq!(value["files"][0]["index"], "A");
    assert_eq!(value["files"][0]["worktree"], " ");
    assert_eq!(value["truncated"], false);
    drop(dir);
}

#[test]
fn commit_file_diff_returns_only_the_selected_path() {
    let (dir, server, token, id) = seeded_server();
    let log = get(server.addr(), &format!("/api/log?repo={id}"), Some(&token));
    let value: serde_json::Value = serde_json::from_str(body_of(&log)).unwrap();
    let oid = value["commits"][0]["oid"].as_str().unwrap();

    let response = get(
        server.addr(),
        &format!("/api/commit/file-diff?repo={id}&oid={oid}&path=src%2Fmain.rs"),
        Some(&token),
    );
    let value: serde_json::Value = serde_json::from_str(body_of(&response)).unwrap();

    assert!(response.starts_with("HTTP/1.1 200"), "got: {response}");
    assert_eq!(value["path"], "src/main.rs");
    assert!(
        value["hunks"].as_array().unwrap().iter().any(|hunk| {
            hunk["file_path"] == "src/main.rs" && !hunk["lines"].as_array().unwrap().is_empty()
        }),
        "expected a diff for just src/main.rs: {value}"
    );
    drop(dir);
}

#[test]
fn commit_file_diff_allows_a_deleted_path_without_worktree_lookup() {
    let (dir, server, token, id) = seeded_server();
    let repo_path = {
        let entry = server.state.session.catalog().get(&id).unwrap();
        entry.path.clone()
    };
    let gone = std::path::Path::new(&repo_path).join("gone.txt");
    std::fs::write(&gone, "before delete\n").unwrap();
    run_git(&repo_path, &["add", "gone.txt"]);
    run_git(&repo_path, &["commit", "-m", "add gone"]);
    run_git(&repo_path, &["rm", "gone.txt"]);
    run_git(&repo_path, &["commit", "-m", "delete gone"]);
    assert!(!gone.exists(), "test precondition: file must be deleted");

    let log = get(server.addr(), &format!("/api/log?repo={id}"), Some(&token));
    let value: serde_json::Value = serde_json::from_str(body_of(&log)).unwrap();
    let oid = value["commits"][0]["oid"].as_str().unwrap();
    let response = get(
        server.addr(),
        &format!("/api/commit/file-diff?repo={id}&oid={oid}&path=gone.txt"),
        Some(&token),
    );
    let value: serde_json::Value = serde_json::from_str(body_of(&response)).unwrap();

    assert!(response.starts_with("HTTP/1.1 200"), "got: {response}");
    assert!(
        value["hunks"]
            .as_array()
            .unwrap()
            .iter()
            .flat_map(|h| h["lines"].as_array().unwrap())
            .any(|line| line["kind"] == "-"),
        "expected a removal line: {value}"
    );
    drop(dir);
}

#[test]
fn commit_file_diff_rejects_traversal_without_requiring_a_worktree_file() {
    let (dir, server, token, id) = seeded_server();
    let log = get(server.addr(), &format!("/api/log?repo={id}"), Some(&token));
    let value: serde_json::Value = serde_json::from_str(body_of(&log)).unwrap();
    let oid = value["commits"][0]["oid"].as_str().unwrap();

    for attack in ["..%2Fsecret", ".git%2Fconfig", "src%2F..%2Fx"] {
        let response = get(
            server.addr(),
            &format!("/api/commit/file-diff?repo={id}&oid={oid}&path={attack}"),
            Some(&token),
        );
        assert!(
            response.starts_with("HTTP/1.1 400"),
            "historical route accepted {attack:?}: {response}"
        );
    }
    drop(dir);
}

#[test]
fn diff_returns_hunks_for_a_changed_file() {
    let (dir, server, token, id) = seeded_server();
    // Mutate the committed file so a worktree diff exists.
    let repo_path = {
        let entry = server.state.session.catalog().get(&id).unwrap();
        entry.path.clone()
    };
    std::fs::write(
        std::path::Path::new(&repo_path).join("src/main.rs"),
        "fn main() { println!(\"hi\"); }\n",
    )
    .unwrap();

    let response = get(
        server.addr(),
        &format!("/api/diff?repo={id}&path=src%2Fmain.rs"),
        Some(&token),
    );
    let value: serde_json::Value = serde_json::from_str(body_of(&response)).unwrap();

    let hunks = value["hunks"].as_array().unwrap();
    assert!(!hunks.is_empty(), "expected a hunk: {value}");
    let kinds: Vec<_> = hunks[0]["lines"]
        .as_array()
        .unwrap()
        .iter()
        .map(|l| l["kind"].as_str().unwrap())
        .collect();
    assert!(
        kinds.contains(&"+") && kinds.contains(&"-"),
        "got: {kinds:?}"
    );
    drop(dir);
}
