use super::*;
use crate::web::common::auth::SESSION_COOKIE;
use ratatui::layout::{Position, Rect};
use ratatui::style::Style;
use std::net::TcpStream;
use std::thread;
use std::time::Duration;
use tungstenite::client::IntoClientRequest;
use tungstenite::Message;

mod helpers;

use helpers::{form_post, http_request, session_token, test_config};

#[test]
fn login_flow_issues_session_and_gates_the_app_page() {
    let server = WebServer::start_from_config(&test_config("swordfish")).unwrap();
    let addr = server.addr();

    let anon = http_request(
        addr,
        "GET / HTTP/1.1\r\nHost: x\r\nConnection: close\r\n\r\n",
    );
    assert!(anon.contains("Sign in"), "login page expected");
    assert!(
        !anon.contains("/vendor/xterm.js"),
        "the terminal app must be gated behind auth"
    );

    let bad = http_request(addr, &form_post("password=nope"));
    assert!(bad.starts_with("HTTP/1.1 401"), "wrong password must 401");

    let ok = http_request(addr, &form_post("password=swordfish"));
    assert!(
        ok.starts_with("HTTP/1.1 303"),
        "correct password must redirect"
    );
    let token = session_token(&ok).expect("a session cookie");

    let app = http_request(
        addr,
        &format!(
            "GET / HTTP/1.1\r\nHost: x\r\nCookie: {SESSION_COOKIE}={token}\r\nConnection: close\r\n\r\n"
        ),
    );
    assert!(
        app.contains("/vendor/xterm.js"),
        "authenticated GET / serves the terminal app"
    );
}

#[test]
fn serves_vendored_renderer_assets() {
    let server = WebServer::start_from_config(&test_config("pw")).unwrap();
    let addr = server.addr();
    let js = http_request(
        addr,
        "GET /vendor/xterm.js HTTP/1.1\r\nHost: x\r\nConnection: close\r\n\r\n",
    );
    assert!(js.starts_with("HTTP/1.1 200"));
    assert!(js.contains("application/javascript"));
}

#[test]
fn logout_revokes_the_session_server_side() {
    // Cookies are not port-isolated, so revocation must happen server-side.
    let server = WebServer::start_from_config(&test_config("swordfish")).unwrap();
    let addr = server.addr();
    let token = session_token(&http_request(addr, &form_post("password=swordfish")))
        .expect("a session cookie");

    let before = http_request(
        addr,
        &format!(
            "GET / HTTP/1.1\r\nHost: x\r\nCookie: {SESSION_COOKIE}={token}\r\nConnection: close\r\n\r\n"
        ),
    );
    assert!(before.contains("/vendor/xterm.js"), "token should be valid");

    http_request(
        addr,
        &format!(
            "GET /logout HTTP/1.1\r\nHost: x\r\nCookie: {SESSION_COOKIE}={token}\r\nConnection: close\r\n\r\n"
        ),
    );

    let after = http_request(
        addr,
        &format!(
            "GET / HTTP/1.1\r\nHost: x\r\nCookie: {SESSION_COOKIE}={token}\r\nConnection: close\r\n\r\n"
        ),
    );
    assert!(
        !after.contains("/vendor/xterm.js"),
        "the token must stop working immediately: {after}"
    );
}

#[test]
fn serves_the_favicon_without_auth() {
    let server = WebServer::start_from_config(&test_config("pw")).unwrap();
    let addr = server.addr();
    // The login page must load this asset before sign-in.
    let svg = http_request(
        addr,
        "GET /crow.svg HTTP/1.1\r\nHost: x\r\nConnection: close\r\n\r\n",
    );
    assert!(svg.starts_with("HTTP/1.1 200"));
    assert!(svg.contains("image/svg+xml"));
    assert!(svg.contains("<svg"));
}

#[test]
fn serves_the_header_mark_without_auth() {
    let server = WebServer::start_from_config(&test_config("pw")).unwrap();
    let addr = server.addr();
    // The login page must load this asset before sign-in.
    let svg = http_request(
        addr,
        "GET /crow-mono.svg HTTP/1.1\r\nHost: x\r\nConnection: close\r\n\r\n",
    );
    assert!(svg.starts_with("HTTP/1.1 200"));
    assert!(svg.contains("image/svg+xml"));
    assert!(svg.contains("<svg"));
}

#[test]
fn websocket_requires_auth() {
    let server = WebServer::start_from_config(&test_config("hunter2")).unwrap();
    let addr = server.addr();
    let resp = http_request(
        addr,
        "GET /ws HTTP/1.1\r\nHost: x\r\nUpgrade: websocket\r\n\
         Connection: Upgrade\r\nSec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\n\
         Sec-WebSocket-Version: 13\r\nConnection: close\r\n\r\n",
    );
    assert!(
        resp.starts_with("HTTP/1.1 401"),
        "unauthenticated WS must 401"
    );
}

#[test]
fn websocket_rejects_cross_origin_even_with_valid_cookie() {
    let server = WebServer::start_from_config(&test_config("hunter2")).unwrap();
    let addr = server.addr();
    let token = session_token(&http_request(addr, &form_post("password=hunter2")))
        .expect("a session cookie");
    // Foreign origins must be refused before the handshake.
    let resp = http_request(
        addr,
        &format!(
            "GET /ws HTTP/1.1\r\nHost: {addr}\r\nOrigin: http://evil.example\r\n\
             Upgrade: websocket\r\nConnection: Upgrade\r\n\
             Sec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\nSec-WebSocket-Version: 13\r\n\
             Cookie: {SESSION_COOKIE}={token}\r\nConnection: close\r\n\r\n"
        ),
    );
    assert!(
        resp.starts_with("HTTP/1.1 403"),
        "cross-origin WS must be forbidden"
    );
}

#[test]
fn websocket_mirrors_frame_and_delivers_input() {
    let mut server = WebServer::start_from_config(&test_config("hunter2")).unwrap();
    let addr = server.addr();
    let token = session_token(&http_request(addr, &form_post("password=hunter2")))
        .expect("a session cookie");

    let stream = TcpStream::connect(addr).unwrap();
    let mut request = format!("ws://{addr}/ws").into_client_request().unwrap();
    request.headers_mut().insert(
        "Cookie",
        format!("{SESSION_COOKIE}={token}").parse().unwrap(),
    );
    let (mut ws, _resp) = tungstenite::client(request, stream).unwrap();
    ws.get_ref()
        .set_read_timeout(Some(Duration::from_millis(200)))
        .unwrap();

    // Retry broadcasts to absorb the connect-vs-register race.
    let mut buffer = Buffer::empty(Rect::new(0, 0, 8, 1));
    buffer.set_string(0, 0, "hello", Style::default());
    let mut resize_seen = false;
    let mut frame = None;
    for _ in 0..100 {
        server.broadcast(&buffer, Some(Position::new(2, 0)));
        match ws.read() {
            Ok(msg) if msg.is_text() => {
                let text = msg.into_text().unwrap();
                assert!(
                    text.contains("\"t\":\"resize\"")
                        && text.contains("\"cols\":8")
                        && text.contains("\"rows\":1"),
                    "resize control message must carry the grid size, got: {text}"
                );
                resize_seen = true;
            }
            Ok(msg) if msg.is_binary() => {
                frame = Some(msg.into_data());
                break;
            }
            Ok(_) => {}
            Err(tungstenite::Error::Io(e))
                if matches!(
                    e.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                ) => {}
            Err(e) => panic!("ws read failed: {e}"),
        }
        thread::sleep(Duration::from_millis(20));
    }
    assert!(
        resize_seen,
        "a new client must receive a resize message first"
    );
    let frame = frame.expect("a broadcast frame within the retry budget");
    assert!(
        frame.windows(5).any(|w| w == b"hello"),
        "the mirrored frame must carry the painted text"
    );
    let cursor_tail = protocol::encode_cursor(Some(Position::new(2, 0)));
    assert!(
        frame.ends_with(&cursor_tail),
        "the frame must end by parking the cursor where the draw left it"
    );

    ws.write(Message::text(r#"{"t":"key","key":"a"}"#)).unwrap();
    ws.flush().unwrap();
    let mut input = Vec::new();
    for _ in 0..100 {
        input = server.drain_input();
        if !input.is_empty() {
            break;
        }
        thread::sleep(Duration::from_millis(20));
    }
    assert_eq!(
        input.len(),
        1,
        "the keypress must be delivered exactly once"
    );
    assert!(matches!(input[0], WebInputEvent::Key(_)));
}
