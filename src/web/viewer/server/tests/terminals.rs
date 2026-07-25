use super::{VIEWER_SESSION_COOKIE, get, request, seeded_server};
use crate::web::viewer::terminal;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::time::Duration;

#[test]
fn the_events_stream_sends_a_status_event() {
    let (dir, server, token, id) = seeded_server();

    let mut stream = TcpStream::connect(server.addr()).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    stream
        .write_all(
            format!(
                "GET /api/events?repo={id} HTTP/1.1\r\nHost: 127.0.0.1\r\n\
                 Cookie: {VIEWER_SESSION_COOKIE}={token}\r\n\r\n"
            )
            .as_bytes(),
        )
        .unwrap();

    // Read until the first dispatched event or the read budget runs out.
    let mut seen = String::new();
    let mut chunk = [0u8; 2048];
    while !seen.contains("event: status") {
        match stream.read(&mut chunk) {
            Ok(0) | Err(_) => break,
            Ok(n) => seen.push_str(&String::from_utf8_lossy(&chunk[..n])),
        }
    }

    assert!(
        seen.starts_with("HTTP/1.1 200"),
        "expected a streaming head: {seen}"
    );
    assert!(
        seen.contains("text/event-stream"),
        "expected an SSE content type: {seen}"
    );
    assert!(!seen.contains("Content-Length"), "SSE must not declare one");
    assert!(seen.contains("event: status"), "no status event: {seen}");
    drop(dir);
}

#[test]
fn the_terminal_socket_creates_a_pane_and_streams_its_output() {
    use tungstenite::client::IntoClientRequest;

    let (dir, server, token, id) = seeded_server();
    let mut request = format!("ws://{}/ws/term?repo={id}", server.addr())
        .into_client_request()
        .unwrap();
    request.headers_mut().insert(
        "Cookie",
        format!("{VIEWER_SESSION_COOKIE}={token}").parse().unwrap(),
    );
    let (mut ws, _) = tungstenite::connect(request).expect("terminal upgrade");

    ws.send(tungstenite::Message::Text(
        r#"{"type":"create","rows":24,"cols":80}"#.into(),
    ))
    .unwrap();

    // Expect created control frames, then real PTY bytes tagged with a pane
    // id — proving the multiplexing round-trips end to end. More than one
    // pane can appear: the first connect also spawns the default startup
    // terminal, so track every announced pane and require output for one.
    let mut created = std::collections::HashSet::new();
    let mut saw_output = false;
    for _ in 0..40 {
        match ws.read() {
            Ok(tungstenite::Message::Text(text)) if text.contains("created") => {
                let value: serde_json::Value = serde_json::from_str(&text).unwrap();
                if let Some(pane) = value["pane"].as_u64() {
                    created.insert(pane as u32);
                }
            }
            Ok(tungstenite::Message::Binary(bytes)) => {
                let (pane, data) = terminal::decode_output(&bytes).expect("a tagged frame");
                assert!(created.contains(&pane), "output for an unannounced pane");
                if !data.is_empty() {
                    saw_output = true;
                    break;
                }
            }
            Ok(_) => {}
            Err(_) => break,
        }
    }

    assert!(!created.is_empty(), "no created frame");
    assert!(saw_output, "no PTY output reached the socket");
    drop(dir);
}

#[test]
fn the_terminal_socket_requires_auth_and_a_known_repo() {
    let (dir, server, token, _id) = seeded_server();

    let anon = get(server.addr(), "/ws/term?repo=r1", None);
    assert!(anon.starts_with("HTTP/1.1 401"), "got: {anon}");

    // Authenticated but unknown: refused before any upgrade happens.
    let unknown = request(
        server.addr(),
        &format!(
            "GET /ws/term?repo=r9999 HTTP/1.1\r\nHost: 127.0.0.1\r\nUpgrade: websocket\r\n\
             Connection: Upgrade\r\nSec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\n\
             Sec-WebSocket-Version: 13\r\nCookie: {VIEWER_SESSION_COOKIE}={token}\r\n\r\n"
        ),
    );
    assert!(unknown.starts_with("HTTP/1.1 404"), "got: {unknown}");
    drop(dir);
}

#[test]
fn the_events_stream_requires_auth_and_a_known_repo() {
    let (dir, server, token, _id) = seeded_server();

    let anon = get(server.addr(), "/api/events?repo=r1", None);
    assert!(anon.starts_with("HTTP/1.1 401"), "got: {anon}");

    let unknown = get(server.addr(), "/api/events?repo=r9999", Some(&token));
    assert!(unknown.starts_with("HTTP/1.1 404"), "got: {unknown}");
    drop(dir);
}
