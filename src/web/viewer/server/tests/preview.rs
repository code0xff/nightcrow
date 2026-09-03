//! What `/api/preview` promises: the file itself, under a policy that lets
//! its inline scripts run while keeping the document an unauthenticated
//! nobody that can reach no host — and only when it is loaded as a frame.

use super::{VIEWER_SESSION_COOKIE, body_of, get, request, run_git, seeded_server};
use std::net::SocketAddr;

/// A `GET` that carries a given `Sec-Fetch-Dest`, the way a browser marks the
/// destination of a request it initiates.
fn get_with_dest(addr: SocketAddr, path: &str, token: &str, dest: &str) -> String {
    request(
        addr,
        &format!(
            "GET {path} HTTP/1.1\r\nHost: 127.0.0.1\r\n\
Cookie: {VIEWER_SESSION_COOKIE}={token}\r\n\
Sec-Fetch-Dest: {dest}\r\nConnection: close\r\n\r\n"
        ),
    )
}

/// A `GET` marked as a frame embed, the way a browser marks a request it is
/// making to fill an `<iframe>`.
fn get_framed(addr: SocketAddr, path: &str, token: &str) -> String {
    get_with_dest(addr, path, token, "iframe")
}

#[test]
fn a_framed_preview_serves_the_file_under_its_own_policy() {
    let (dir, server, token, id) = seeded_server();
    std::fs::write(
        dir.path().join("deck.html"),
        "<html><body><script>step()</script></body></html>",
    )
    .unwrap();

    let response = get_framed(
        server.addr(),
        &format!("/api/preview?repo={id}&path=deck.html"),
        &token,
    );

    assert!(response.starts_with("HTTP/1.1 200"), "got: {response}");
    assert!(response.contains("Content-Type: text/html"), "{response}");
    // The whole point of the endpoint: an opaque-origin sandbox whose inline
    // scripts run, with every outbound channel shut.
    assert!(
        response.contains("Content-Security-Policy: sandbox allow-scripts;"),
        "{response}"
    );
    assert!(
        response.contains("script-src 'unsafe-inline'"),
        "{response}"
    );
    assert!(response.contains("connect-src 'none'"), "{response}");
    assert!(response.contains("Cache-Control: no-store"), "{response}");
    // The blob oid the editor reads back as the version its edits began from.
    assert!(response.contains("ETag: \""), "{response}");
    assert!(
        body_of(&response).contains("<script>step()</script>"),
        "the document must arrive as it is: {response}"
    );
    drop(dir);
}

#[test]
fn a_top_level_navigation_gets_the_inert_source_not_an_executable_page() {
    // A pasted URL or a link is a top-level navigation (`Sec-Fetch-Dest:
    // document`), not a frame embed. Serving it as text/html would run a
    // repository file as a first-party document on any browser that ignored
    // the CSP sandbox; text/plain cannot execute at all.
    let (dir, server, token, id) = seeded_server();
    std::fs::write(
        dir.path().join("deck.html"),
        "<html><body><script>step()</script></body></html>",
    )
    .unwrap();

    let response = get_with_dest(
        server.addr(),
        &format!("/api/preview?repo={id}&path=deck.html"),
        &token,
        "document",
    );

    assert!(response.starts_with("HTTP/1.1 200"), "got: {response}");
    assert!(response.contains("Content-Type: text/plain"), "{response}");
    assert!(
        !response.contains("Content-Security-Policy: sandbox"),
        "the exec policy must not ride the inert view: {response}"
    );
    drop(dir);
}

#[test]
fn a_request_without_fetch_metadata_gets_the_executable_page() {
    // A plain-HTTP origin (a phone over a LAN or Tailscale address) sends no
    // Sec-Fetch metadata at all, so the embed cannot be told from a top-level
    // load by the header. The gate fails open: the frame gets the executable
    // document — the CSP sandbox is the wall on this path — rather than the raw
    // source, which is what left the whole mobile preview showing only text.
    let (dir, server, token, id) = seeded_server();
    std::fs::write(
        dir.path().join("deck.html"),
        "<html><body><script>step()</script></body></html>",
    )
    .unwrap();

    // `get` sends no Sec-Fetch-Dest.
    let response = get(
        server.addr(),
        &format!("/api/preview?repo={id}&path=deck.html"),
        Some(&token),
    );

    assert!(response.starts_with("HTTP/1.1 200"), "got: {response}");
    assert!(response.contains("Content-Type: text/html"), "{response}");
    assert!(
        response.contains("Content-Security-Policy: sandbox allow-scripts;"),
        "{response}"
    );
    drop(dir);
}

#[test]
fn a_framed_preview_serves_a_commits_version_of_the_file() {
    let (dir, server, token, id) = seeded_server();
    let repo = dir.path().to_path_buf();
    std::fs::write(repo.join("deck.html"), "<p>committed</p>").unwrap();
    run_git(repo.to_str().unwrap(), &["add", "."]);
    run_git(repo.to_str().unwrap(), &["commit", "-m", "deck"]);
    let oid = String::from_utf8(
        std::process::Command::new("git")
            .args(["rev-parse", "HEAD"])
            .current_dir(&repo)
            .output()
            .unwrap()
            .stdout,
    )
    .unwrap();
    // The working tree moves on; the commit's version is what is asked for.
    std::fs::write(repo.join("deck.html"), "<p>since edited</p>").unwrap();

    let response = get_framed(
        server.addr(),
        &format!("/api/preview?repo={id}&path=deck.html&oid={}", oid.trim()),
        &token,
    );

    assert!(response.starts_with("HTTP/1.1 200"), "got: {response}");
    assert!(body_of(&response).contains("committed"), "{response}");
    assert!(!body_of(&response).contains("since edited"), "{response}");
    drop(dir);
}

#[test]
fn the_preview_requires_a_session() {
    let (dir, server, _token, id) = seeded_server();

    let response = get(
        server.addr(),
        &format!("/api/preview?repo={id}&path=deck.html"),
        None,
    );

    assert!(response.starts_with("HTTP/1.1 401"), "got: {response}");
    drop(dir);
}
