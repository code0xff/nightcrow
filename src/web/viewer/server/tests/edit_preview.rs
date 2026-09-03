//! What `/api/preview/edit` promises: the editor POSTs an insert list and the
//! blob oid its parse began from; the server splices the inserts into that exact
//! version of the file and hands back a one-time token the frame loads under the
//! sandbox policy — and only that version, only once.

use super::{body_of, get, post, seeded_server};

fn blob_oid(bytes: &[u8]) -> String {
    git2::Oid::hash_object(git2::ObjectType::Blob, bytes)
        .unwrap()
        .to_string()
}

fn stash_body(inserts: serde_json::Value, base_hash: &str) -> String {
    serde_json::json!({ "inserts": inserts, "base_hash": base_hash }).to_string()
}

#[test]
fn an_assembled_preview_is_served_once_under_the_sandbox_policy() {
    let (dir, server, token, id) = seeded_server();
    let source = "<html><head></head><body><p>Hi</p></body></html>";
    std::fs::write(dir.path().join("deck.html"), source).unwrap();

    let response = post(
        server.addr(),
        &format!("/api/preview/edit?repo={id}&path=deck.html"),
        &stash_body(
            serde_json::json!([{ "at": 0, "text": "<!--edited-->" }]),
            &blob_oid(source.as_bytes()),
        ),
        Some(&token),
    );
    assert!(response.starts_with("HTTP/1.1 200"), "got: {response}");
    let stash: serde_json::Value = serde_json::from_str(body_of(&response)).unwrap();
    let preview_token = stash["token"].as_str().unwrap();

    let served = get(
        server.addr(),
        &format!("/api/preview/edit?token={preview_token}"),
        Some(&token),
    );
    assert!(served.starts_with("HTTP/1.1 200"), "got: {served}");
    assert!(served.contains("Content-Type: text/html"), "{served}");
    assert!(
        served.contains("Content-Security-Policy: sandbox allow-scripts;"),
        "{served}"
    );
    assert!(body_of(&served).starts_with("<!--edited-->"), "{served}");

    // Single use: the token is spent.
    let again = get(
        server.addr(),
        &format!("/api/preview/edit?token={preview_token}"),
        Some(&token),
    );
    assert!(again.starts_with("HTTP/1.1 404"), "got: {again}");
    drop(dir);
}

#[test]
fn a_stale_base_hash_is_refused_with_the_current_oid() {
    let (dir, server, token, id) = seeded_server();
    let source = "<p>current</p>\n";
    std::fs::write(dir.path().join("deck.html"), source).unwrap();

    let response = post(
        server.addr(),
        &format!("/api/preview/edit?repo={id}&path=deck.html"),
        &stash_body(
            serde_json::json!([{ "at": 0, "text": "x" }]),
            &blob_oid(b"an older version"),
        ),
        Some(&token),
    );
    assert!(response.starts_with("HTTP/1.1 409"), "got: {response}");
    let body: serde_json::Value = serde_json::from_str(body_of(&response)).unwrap();
    assert_eq!(body["error"], "stale");
    assert_eq!(body["currentHash"], blob_oid(source.as_bytes()));
    drop(dir);
}

#[test]
fn an_insert_offset_past_the_source_is_refused() {
    let (dir, server, token, id) = seeded_server();
    let source = "<p>x</p>";
    std::fs::write(dir.path().join("deck.html"), source).unwrap();

    let response = post(
        server.addr(),
        &format!("/api/preview/edit?repo={id}&path=deck.html"),
        &stash_body(
            serde_json::json!([{ "at": 9999, "text": "x" }]),
            &blob_oid(source.as_bytes()),
        ),
        Some(&token),
    );
    assert!(response.starts_with("HTTP/1.1 400"), "got: {response}");
    drop(dir);
}

#[test]
fn a_top_level_navigation_to_an_assembled_preview_is_refused() {
    // The frame is the only context it is for; a pasted link would otherwise
    // spend the token running markers and an agent as a first-party document on
    // any browser that ignored the CSP sandbox.
    let (dir, server, token, id) = seeded_server();
    let source = "<p>x</p>";
    std::fs::write(dir.path().join("deck.html"), source).unwrap();
    let stash = post(
        server.addr(),
        &format!("/api/preview/edit?repo={id}&path=deck.html"),
        &stash_body(serde_json::json!([]), &blob_oid(source.as_bytes())),
        Some(&token),
    );
    let preview_token =
        serde_json::from_str::<serde_json::Value>(body_of(&stash)).unwrap()["token"]
            .as_str()
            .unwrap()
            .to_string();

    let navigated = super::request(
        server.addr(),
        &format!(
            "GET /api/preview/edit?token={preview_token} HTTP/1.1\r\nHost: 127.0.0.1\r\n\
Cookie: {}={token}\r\nSec-Fetch-Dest: document\r\nConnection: close\r\n\r\n",
            super::VIEWER_SESSION_COOKIE
        ),
    );
    assert!(navigated.starts_with("HTTP/1.1 403"), "got: {navigated}");

    // Refused without spending the token: the frame can still load it.
    let framed = get(
        server.addr(),
        &format!("/api/preview/edit?token={preview_token}"),
        Some(&token),
    );
    assert!(framed.starts_with("HTTP/1.1 200"), "got: {framed}");
    drop(dir);
}

#[test]
fn an_unknown_preview_token_is_not_found() {
    let (dir, server, token, _id) = seeded_server();
    let response = get(
        server.addr(),
        "/api/preview/edit?token=deadbeef",
        Some(&token),
    );
    assert!(response.starts_with("HTTP/1.1 404"), "got: {response}");
    drop(dir);
}

#[test]
fn assembling_a_preview_requires_a_session() {
    let (dir, server, _token, id) = seeded_server();
    std::fs::write(dir.path().join("deck.html"), "<p>x</p>").unwrap();

    let response = post(
        server.addr(),
        &format!("/api/preview/edit?repo={id}&path=deck.html"),
        &stash_body(serde_json::json!([]), &blob_oid(b"<p>x</p>")),
        None,
    );
    assert!(response.starts_with("HTTP/1.1 401"), "got: {response}");
    drop(dir);
}
