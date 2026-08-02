use super::{body_of, get, seeded_server};
use crate::test_util::run_git;

/// The file's text, flattened out of the highlighted spans `FileDto` carries.
fn file_text(value: &serde_json::Value) -> String {
    value["lines"]
        .as_array()
        .expect("a file payload has lines")
        .iter()
        .flat_map(|line| line.as_array().expect("a line is spans").iter())
        .map(|span| span["t"].as_str().unwrap_or_default())
        .collect()
}

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

/// The route answers about the file it was asked for, not the directory the
/// name happens to be a prefix of.
///
/// git matches a directory pathspec against everything beneath it, so this once
/// answered with every changed file under `src` — hunks from several files,
/// under the one name the caller supplied. Pinned at the route because that is
/// where the name in the answer is set.
#[test]
fn diff_of_a_directory_is_not_the_files_under_it() {
    let (dir, server, token, id) = seeded_server();
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
        &format!("/api/diff?repo={id}&path=src"),
        Some(&token),
    );
    let value: serde_json::Value = serde_json::from_str(body_of(&response)).unwrap();

    assert!(response.starts_with("HTTP/1.1 200"), "got: {response}");
    assert_eq!(value["path"], "src");
    assert!(
        value["hunks"].as_array().unwrap().is_empty(),
        "a directory answered with the files under it: {value}"
    );
    drop(dir);
}

#[test]
fn commit_file_serves_the_contents_as_of_that_commit() {
    // Not the working tree's: the point of reading a file from the log is to
    // see what it was, and the two differ the moment anything is edited after.
    let (dir, server, token, id) = seeded_server();
    let repo_path = {
        let entry = server.state.session.catalog().get(&id).unwrap();
        entry.path.clone()
    };
    let file = std::path::Path::new(&repo_path).join("history.txt");
    std::fs::write(&file, "as committed\n").unwrap();
    run_git(&repo_path, &["add", "history.txt"]);
    run_git(&repo_path, &["commit", "-m", "add history"]);
    std::fs::write(&file, "edited since\n").unwrap();

    let log = get(server.addr(), &format!("/api/log?repo={id}"), Some(&token));
    let value: serde_json::Value = serde_json::from_str(body_of(&log)).unwrap();
    let oid = value["commits"][0]["oid"].as_str().unwrap();
    let response = get(
        server.addr(),
        &format!("/api/commit/file?repo={id}&oid={oid}&path=history.txt"),
        Some(&token),
    );
    let value: serde_json::Value = serde_json::from_str(body_of(&response)).unwrap();

    assert!(response.starts_with("HTTP/1.1 200"), "got: {response}");
    assert_eq!(value["path"], "history.txt");
    let text = file_text(&value);
    assert!(
        text.contains("as committed"),
        "expected the committed contents: {text:?}"
    );
    assert!(
        !text.contains("edited since"),
        "the working tree must not leak into a historical read: {text:?}"
    );
    drop(dir);
}

#[test]
fn commit_file_reads_a_deleted_path_from_the_commit_that_removed_it() {
    // A deleted path is not in its own commit's tree. The server works that out
    // rather than being told, so a client cannot be wrong about it.
    let (dir, server, token, id) = seeded_server();
    let repo_path = {
        let entry = server.state.session.catalog().get(&id).unwrap();
        entry.path.clone()
    };
    let gone = std::path::Path::new(&repo_path).join("gone.txt");
    std::fs::write(&gone, "what it held\n").unwrap();
    run_git(&repo_path, &["add", "gone.txt"]);
    run_git(&repo_path, &["commit", "-m", "add gone"]);
    run_git(&repo_path, &["rm", "gone.txt"]);
    run_git(&repo_path, &["commit", "-m", "delete gone"]);

    let log = get(server.addr(), &format!("/api/log?repo={id}"), Some(&token));
    let value: serde_json::Value = serde_json::from_str(body_of(&log)).unwrap();
    let oid = value["commits"][0]["oid"].as_str().unwrap();
    let response = get(
        server.addr(),
        &format!("/api/commit/file?repo={id}&oid={oid}&path=gone.txt"),
        Some(&token),
    );
    let value: serde_json::Value = serde_json::from_str(body_of(&response)).unwrap();

    assert!(response.starts_with("HTTP/1.1 200"), "got: {response}");
    let text = file_text(&value);
    assert!(
        text.contains("what it held"),
        "expected the contents the commit removed: {text:?}"
    );
    drop(dir);
}

#[test]
fn commit_file_refuses_a_path_that_commit_never_had() {
    let (dir, server, token, id) = seeded_server();
    let log = get(server.addr(), &format!("/api/log?repo={id}"), Some(&token));
    let value: serde_json::Value = serde_json::from_str(body_of(&log)).unwrap();
    let oid = value["commits"][0]["oid"].as_str().unwrap();

    let response = get(
        server.addr(),
        &format!("/api/commit/file?repo={id}&oid={oid}&path=never%2Fexisted.rs"),
        Some(&token),
    );

    assert!(
        !response.starts_with("HTTP/1.1 200"),
        "a path the commit never had must not answer with contents: {response}"
    );
    drop(dir);
}
