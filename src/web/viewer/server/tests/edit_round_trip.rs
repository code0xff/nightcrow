//! The whole server side of one editing session, in the order the editor walks
//! it: read the file's exact bytes and the version they hash to, have the
//! preview assembled against that version, then write the edit back against the
//! same version — and land a change the size of the edit, not of the file.

use super::{body_of, post, seeded_server};
use std::net::SocketAddr;

const DECK: &str =
    "<html><head><title>Deck</title></head>\n<body>\n<p>first</p>\n<p>second</p>\n</body></html>\n";

/// A framed `GET`, the way the preview iframe asks.
fn get_framed(addr: SocketAddr, path: &str, token: &str) -> String {
    super::request(
        addr,
        &format!(
            "GET {path} HTTP/1.1\r\nHost: 127.0.0.1\r\n\
Cookie: {}={token}\r\nSec-Fetch-Dest: iframe\r\nConnection: close\r\n\r\n",
            super::VIEWER_SESSION_COOKIE
        ),
    )
}

/// The `ETag` value, unquoted — the blob oid the editor carries as its base.
fn etag_of(response: &str) -> String {
    response
        .lines()
        .find_map(|line| line.strip_prefix("ETag: "))
        .expect("a preview carries an ETag")
        .trim()
        .trim_matches('"')
        .to_string()
}

#[test]
fn an_edit_round_trip_changes_only_the_edited_block() {
    let (dir, server, session, id) = seeded_server();
    std::fs::write(dir.path().join("deck.html"), DECK).unwrap();

    // 1. The editor reads the bytes it will parse, and the version they are.
    let preview = get_framed(
        server.addr(),
        &format!("/api/preview?repo={id}&path=deck.html"),
        &session,
    );
    assert!(preview.starts_with("HTTP/1.1 200"), "got: {preview}");
    assert_eq!(
        body_of(&preview),
        DECK,
        "the preview serves the exact bytes"
    );
    let base = etag_of(&preview);

    // 2. The preview is assembled against that version: a marker on each
    //    paragraph's opening tag, at the byte offset the parse found.
    let first = DECK.find("<p>").unwrap() + "<p".len();
    let second = DECK.rfind("<p>").unwrap() + "<p".len();
    let stash = post(
        server.addr(),
        &format!("/api/preview/edit?repo={id}&path=deck.html"),
        &serde_json::json!({
            "inserts": [
                { "at": first, "text": " data-ne-id=\"0\"" },
                { "at": second, "text": " data-ne-id=\"1\"" },
            ],
            "base_hash": base,
        })
        .to_string(),
        Some(&session),
    );
    assert!(stash.starts_with("HTTP/1.1 200"), "got: {stash}");
    let token = serde_json::from_str::<serde_json::Value>(body_of(&stash)).unwrap()["token"]
        .as_str()
        .unwrap()
        .to_string();

    let assembled = get_framed(
        server.addr(),
        &format!("/api/preview/edit?token={token}"),
        &session,
    );
    assert!(assembled.starts_with("HTTP/1.1 200"), "got: {assembled}");
    assert!(
        body_of(&assembled).contains("<p data-ne-id=\"0\">first</p>"),
        "the markers land on the opening tags: {}",
        body_of(&assembled)
    );
    // Preview furniture never reaches the file — the save starts from the source.
    assert!(
        !std::fs::read_to_string(dir.path().join("deck.html"))
            .unwrap()
            .contains("data-ne-id")
    );

    // 3. The edit goes back against the same version: the original with one
    //    block's inner replaced, which is what the client's patch pass builds.
    let edited = DECK.replace("<p>second</p>", "<p>edited</p>");
    let saved = post(
        server.addr(),
        &format!("/api/file?repo={id}&path=deck.html"),
        &serde_json::json!({ "content": edited, "base_hash": base, "force": false }).to_string(),
        Some(&session),
    );
    assert!(saved.starts_with("HTTP/1.1 200"), "got: {saved}");

    // The file differs from the original by exactly the one line.
    let on_disk = std::fs::read_to_string(dir.path().join("deck.html")).unwrap();
    let changed: Vec<_> = DECK
        .lines()
        .zip(on_disk.lines())
        .filter(|(before, after)| before != after)
        .collect();
    assert_eq!(changed, vec![("<p>second</p>", "<p>edited</p>")]);
    assert_eq!(DECK.lines().count(), on_disk.lines().count());
    drop(dir);
}

#[test]
fn a_second_editor_working_from_an_older_version_is_refused_at_both_gates() {
    let (dir, server, session, id) = seeded_server();
    std::fs::write(dir.path().join("deck.html"), DECK).unwrap();
    let stale = {
        let preview = get_framed(
            server.addr(),
            &format!("/api/preview?repo={id}&path=deck.html"),
            &session,
        );
        etag_of(&preview)
    };
    // The file moves on under the open session.
    std::fs::write(dir.path().join("deck.html"), "<p>someone else</p>\n").unwrap();

    // Assembling a preview against the version that is gone would place markers
    // on shifted bytes, so it is refused rather than rendered wrong.
    let stash = post(
        server.addr(),
        &format!("/api/preview/edit?repo={id}&path=deck.html"),
        &serde_json::json!({ "inserts": [], "base_hash": stale }).to_string(),
        Some(&session),
    );
    assert!(stash.starts_with("HTTP/1.1 409"), "got: {stash}");

    // And so is the save, so the other change is not clobbered unasked.
    let saved = post(
        server.addr(),
        &format!("/api/file?repo={id}&path=deck.html"),
        &serde_json::json!({ "content": DECK, "base_hash": stale, "force": false }).to_string(),
        Some(&session),
    );
    assert!(saved.starts_with("HTTP/1.1 409"), "got: {saved}");
    assert_eq!(
        std::fs::read_to_string(dir.path().join("deck.html")).unwrap(),
        "<p>someone else</p>\n"
    );
    drop(dir);
}
