use super::*;
use crate::config::WebMirrorConfig;
use crate::web::common::auth::SESSION_COOKIE;
use ratatui::layout::{Position, Rect};
use ratatui::style::Style;
use std::io::Read;
use std::io::Write;
use std::net::SocketAddr;
use std::net::TcpStream;
use std::thread;
use std::time::Duration;
use tungstenite::client::IntoClientRequest;
use tungstenite::Message;


fn test_config(password: &str) -> WebMirrorConfig {
    WebMirrorConfig {
        enabled: true,
        bind: "127.0.0.1".into(),
        // Port 0 asks the OS for a free ephemeral port.
        port: 0,
        password: Some(password.into()),
        hashed_password: None,
    }
}

/// Send a raw HTTP request and read the full response (server closes the
/// connection after each response).
fn http_request(addr: SocketAddr, raw: &str) -> String {
    let mut stream = TcpStream::connect(addr).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    stream.write_all(raw.as_bytes()).unwrap();
    let mut buf = Vec::new();
    // Reads until the server closes the socket (Connection: close).
    let _ = stream.read_to_end(&mut buf);
    String::from_utf8_lossy(&buf).into_owned()
}

fn form_post(body: &str) -> String {
    format!(
        "POST /login HTTP/1.1\r\nHost: x\r\n\
         Content-Type: application/x-www-form-urlencoded\r\n\
         Content-Length: {}\r\nConnection: close\r\n\r\n{}",
        body.len(),
        body
    )
}

fn session_token(response: &str) -> Option<String> {
    for line in response.lines() {
        if let Some(value) = line.strip_prefix("Set-Cookie: ")
            && let Some(rest) = value.strip_prefix(&format!("{SESSION_COOKIE}="))
        {
            let token = rest.split(';').next()?.trim();
            if !token.is_empty() {
                return Some(token.to_string());
            }
        }
    }
    None
}

#[test]
fn login_flow_issues_session_and_gates_the_app_page() {
    let server = WebServer::start_from_config(&test_config("swordfish")).unwrap();
    let addr = server.addr();

    // Unauthenticated GET / serves the login page, not the app.
    let anon = http_request(
        addr,
        "GET / HTTP/1.1\r\nHost: x\r\nConnection: close\r\n\r\n",
    );
    assert!(anon.contains("Sign in"), "login page expected");
    assert!(
        !anon.contains("/vendor/xterm.js"),
        "the terminal app must be gated behind auth"
    );

    // Wrong password is rejected.
    let bad = http_request(addr, &form_post("password=nope"));
    assert!(bad.starts_with("HTTP/1.1 401"), "wrong password must 401");

    // Correct password issues a session cookie via a redirect.
    let ok = http_request(addr, &form_post("password=swordfish"));
    assert!(
        ok.starts_with("HTTP/1.1 303"),
        "correct password must redirect"
    );
    let token = session_token(&ok).expect("a session cookie");

    // The cookie unlocks the app page.
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
fn serves_the_favicon_without_auth() {
    let server = WebServer::start_from_config(&test_config("pw")).unwrap();
    let addr = server.addr();
    // The login page references /crow.svg, so it must load before sign-in.
    let svg = http_request(
        addr,
        "GET /crow.svg HTTP/1.1\r\nHost: x\r\nConnection: close\r\n\r\n",
    );
    assert!(svg.starts_with("HTTP/1.1 200"));
    assert!(svg.contains("image/svg+xml"));
    assert!(svg.contains("<svg"));
}

#[test]
fn websocket_requires_auth() {
    let server = WebServer::start_from_config(&test_config("hunter2")).unwrap();
    let addr = server.addr();
    // A WS upgrade without a session cookie is refused before the handshake.
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
    // A valid cookie but a foreign Origin (cross-site WebSocket hijack
    // attempt) must be refused before the handshake.
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

    // Open an authenticated WebSocket.
    let stream = TcpStream::connect(addr).unwrap();
    let mut request = format!("ws://{addr}/ws").into_client_request().unwrap();
    request.headers_mut().insert(
        "Cookie",
        format!("{SESSION_COOKIE}={token}").parse().unwrap(),
    );
    let (mut ws, _resp) = tungstenite::client(request, stream).unwrap();
    // Poll for frames without blocking the retry loop below.
    ws.get_ref()
        .set_read_timeout(Some(Duration::from_millis(200)))
        .unwrap();

    // Broadcast a frame; retry to absorb the connect-vs-register race. A new
    // client receives a resize control message (text) then the full frame
    // (binary).
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

    // Input sent from the browser reaches the main loop's drain.
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
