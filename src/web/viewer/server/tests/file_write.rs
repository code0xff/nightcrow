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

/// Post a body the server will refuse unread, tolerating the broken pipe that
/// follows: it answers and closes rather than draining megabytes it has already
/// decided not to use.
fn send_oversized(addr: std::net::SocketAddr, path: &str, body: &str, token: &str) -> String {
    use std::io::{Read, Write};
    let mut stream = std::net::TcpStream::connect(addr).unwrap();
    stream
        .set_read_timeout(Some(std::time::Duration::from_secs(30)))
        .unwrap();
    let request = format!(
        "POST {path} HTTP/1.1\r\nHost: 127.0.0.1\r\n\
         Content-Type: application/json\r\n\
         Cookie: {}={token}\r\n\
         Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
        super::VIEWER_SESSION_COOKIE,
        body.len()
    );
    let _ = stream.write_all(request.as_bytes());
    let mut buf = Vec::new();
    let _ = stream.read_to_end(&mut buf);
    String::from_utf8_lossy(&buf).into_owned()
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
fn a_document_past_the_general_body_cap_still_saves() {
    // The whole point of the editor is HTML artifacts, which run well past the
    // 64KB every other route is held to. Saving one used to arrive truncated.
    let (dir, server, token, id) = seeded_server();
    let before = format!("<p>{}</p>\n", "a".repeat(200_000));
    std::fs::write(dir.path().join("big.html"), &before).unwrap();
    let after = format!("<p>{}</p>\n", "b".repeat(200_000));

    let response = post(
        server.addr(),
        &format!("/api/file?repo={id}&path=big.html"),
        &save_body(&after, &blob_oid(before.as_bytes()), false),
        Some(&token),
    );

    assert!(response.starts_with("HTTP/1.1 200"), "got: {response}");
    assert_eq!(
        std::fs::read_to_string(dir.path().join("big.html")).unwrap(),
        after
    );
    drop(dir);
}

#[test]
fn a_body_past_the_write_cap_is_refused_whole_rather_than_cut_to_fit() {
    // A truncated body can still parse, and writing the part that arrived would
    // put a half a file on disk under the name of a whole one.
    let (dir, server, token, id) = seeded_server();
    let before = "<p>small</p>\n";
    std::fs::write(dir.path().join("page.html"), before).unwrap();
    let huge = "c".repeat(crate::web::viewer::limits::MAX_FILE_WRITE_BYTES + 1);
    let response = send_oversized(
        server.addr(),
        &format!("/api/file?repo={id}&path=page.html"),
        &save_body(&huge, &blob_oid(before.as_bytes()), false),
        &token,
    );

    // The server answers and closes without draining the upload, so the write
    // may break before the whole body is out; either way it must not be acted on.
    assert!(
        response.is_empty() || response.starts_with("HTTP/1.1 413"),
        "got: {response}"
    );
    assert_eq!(
        std::fs::read_to_string(dir.path().join("page.html")).unwrap(),
        before,
        "the refusal wrote nothing"
    );
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
