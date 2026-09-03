//! What `POST /api/file` promises: it overwrites a working-tree file with
//! edited contents, but only inside the worktree, only over a file that is
//! there, and only when the caller's base version still matches what is on
//! disk — unless it says to force past that.

use super::{body_of, post, seeded_server};

/// The git blob oid of some bytes, the same identity the endpoint compares by.
fn blob_oid(bytes: &[u8]) -> String {
    git2::Oid::hash_object(git2::ObjectType::Blob, bytes)
        .unwrap()
        .to_string()
}

fn save_body(content: &str, base_hash: &str, force: bool) -> String {
    serde_json::json!({ "content": content, "base_hash": base_hash, "force": force }).to_string()
}

#[test]
fn saving_a_file_overwrites_it_and_returns_the_new_hash() {
    let (dir, server, token, id) = seeded_server();
    let before = "<p>before</p>\n";
    std::fs::write(dir.path().join("page.html"), before).unwrap();

    let response = post(
        server.addr(),
        &format!("/api/file?repo={id}&path=page.html"),
        &save_body("<p>after</p>\n", &blob_oid(before.as_bytes()), false),
        Some(&token),
    );

    assert!(response.starts_with("HTTP/1.1 200"), "got: {response}");
    let body: serde_json::Value = serde_json::from_str(body_of(&response)).unwrap();
    assert_eq!(body["hash"], blob_oid("<p>after</p>\n".as_bytes()));
    let on_disk = std::fs::read_to_string(dir.path().join("page.html")).unwrap();
    assert_eq!(on_disk, "<p>after</p>\n");
    drop(dir);
}

#[test]
fn a_stale_base_hash_is_refused_with_the_current_oid() {
    let (dir, server, token, id) = seeded_server();
    let current = "<p>moved on</p>\n";
    std::fs::write(dir.path().join("page.html"), current).unwrap();

    // A base the file no longer hashes to: an edit begun from an older version.
    let response = post(
        server.addr(),
        &format!("/api/file?repo={id}&path=page.html"),
        &save_body("<p>my edit</p>\n", &blob_oid(b"something older"), false),
        Some(&token),
    );

    assert!(response.starts_with("HTTP/1.1 409"), "got: {response}");
    let body: serde_json::Value = serde_json::from_str(body_of(&response)).unwrap();
    assert_eq!(body["error"], "stale");
    assert_eq!(body["currentHash"], blob_oid(current.as_bytes()));
    // The refusal wrote nothing.
    assert_eq!(
        std::fs::read_to_string(dir.path().join("page.html")).unwrap(),
        current
    );
    drop(dir);
}

#[test]
fn force_overwrites_a_stale_file() {
    let (dir, server, token, id) = seeded_server();
    std::fs::write(dir.path().join("page.html"), "<p>moved on</p>\n").unwrap();

    let response = post(
        server.addr(),
        &format!("/api/file?repo={id}&path=page.html"),
        &save_body("<p>forced</p>\n", &blob_oid(b"something older"), true),
        Some(&token),
    );

    assert!(response.starts_with("HTTP/1.1 200"), "got: {response}");
    assert_eq!(
        std::fs::read_to_string(dir.path().join("page.html")).unwrap(),
        "<p>forced</p>\n"
    );
    drop(dir);
}

#[test]
fn a_path_outside_the_worktree_is_refused() {
    let (dir, server, token, id) = seeded_server();

    let response = post(
        server.addr(),
        &format!("/api/file?repo={id}&path=../escape.html"),
        &save_body("x", &blob_oid(b"x"), true),
        Some(&token),
    );

    assert!(response.starts_with("HTTP/1.1 400"), "got: {response}");
    assert!(
        !dir.path().parent().unwrap().join("escape.html").exists(),
        "the write must not have escaped the worktree"
    );
    drop(dir);
}

#[test]
fn a_missing_file_is_not_created() {
    let (dir, server, token, id) = seeded_server();

    // Editing targets a file that is there; a path that is not is rejected by
    // the worktree gate rather than brought into being.
    let response = post(
        server.addr(),
        &format!("/api/file?repo={id}&path=absent.html"),
        &save_body("<p>new</p>\n", &blob_oid(b""), true),
        Some(&token),
    );

    assert!(response.starts_with("HTTP/1.1 400"), "got: {response}");
    assert!(!dir.path().join("absent.html").exists(), "{response}");
    drop(dir);
}

#[test]
fn writing_requires_a_session() {
    let (dir, server, _token, id) = seeded_server();
    std::fs::write(dir.path().join("page.html"), "<p>x</p>\n").unwrap();

    let response = post(
        server.addr(),
        &format!("/api/file?repo={id}&path=page.html"),
        &save_body("<p>y</p>\n", &blob_oid(b"<p>x</p>\n"), false),
        None,
    );

    assert!(response.starts_with("HTTP/1.1 401"), "got: {response}");
    drop(dir);
}
