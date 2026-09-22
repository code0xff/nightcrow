//! What `POST /api/paste-image` promises at its edges: it is behind the
//! session like every other API route, it refuses anything that is not one of
//! the clipboard image formats, and a body over the ceiling is refused
//! unread rather than written as a truncated file.
//!
//! The write itself is covered where it can be aimed at a temp directory
//! instead of the runner's home — see `mutations::paste_image`.

use super::{body_of, login, post, seeded_server, server};

#[test]
fn pasting_an_image_requires_a_session() {
    let (dir, path) = crate::test_util::make_repo();
    let server = server(&[path]);

    let response = post(server.addr(), "/api/paste-image", "whatever", None);

    assert!(response.starts_with("HTTP/1.1 401"), "got: {response}");
    drop(dir);
}

#[test]
fn a_body_that_is_not_a_known_image_format_is_refused() {
    let (dir, server, token, _) = seeded_server();

    let response = post(
        server.addr(),
        "/api/paste-image",
        "<html>not an image</html>",
        Some(&token),
    );

    assert!(response.starts_with("HTTP/1.1 415"), "got: {response}");
    assert!(body_of(&response).contains("image"), "got: {response}");
    drop(dir);
}

/// Declare an over-limit body and send none of it. The refusal is decided
/// from `Content-Length` alone, so nothing needs to be transmitted — and a
/// client that really did send ten megabytes would get its write cut short by
/// the server answering and closing, which is the documented behaviour but
/// makes for a racy test.
fn declare_oversized(addr: std::net::SocketAddr, path: &str, length: usize, token: &str) -> String {
    use std::io::{Read, Write};
    let mut stream = std::net::TcpStream::connect(addr).unwrap();
    stream
        .set_read_timeout(Some(std::time::Duration::from_secs(30)))
        .unwrap();
    let request = format!(
        "POST {path} HTTP/1.1\r\nHost: 127.0.0.1\r\n\
         Content-Type: application/octet-stream\r\n\
         Cookie: {}={token}\r\n\
         Content-Length: {length}\r\nConnection: close\r\n\r\n",
        super::VIEWER_SESSION_COOKIE,
    );
    stream.write_all(request.as_bytes()).unwrap();
    let mut buf = Vec::new();
    let _ = stream.read_to_end(&mut buf);
    String::from_utf8_lossy(&buf).into_owned()
}

#[test]
fn a_body_over_the_ceiling_is_refused_unread() {
    let (dir, path) = crate::test_util::make_repo();
    let server = server(&[path]);
    let token = login(server.addr());

    let response = declare_oversized(
        server.addr(),
        "/api/paste-image",
        crate::web::viewer::limits::MAX_PASTE_IMAGE_BYTES + 1,
        &token,
    );

    assert!(response.starts_with("HTTP/1.1 413"), "got: {response}");
    drop(dir);
}
