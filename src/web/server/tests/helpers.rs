use crate::config::WebMirrorConfig;
use crate::web::common::auth::SESSION_COOKIE;
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::time::Duration;

pub(super) fn test_config(password: &str) -> WebMirrorConfig {
    WebMirrorConfig {
        enabled: true,
        bind: "127.0.0.1".into(),
        port: 0,
        password: Some(password.into()),
        hashed_password: None,
    }
}

pub(super) fn http_request(addr: SocketAddr, raw: &str) -> String {
    let mut stream = TcpStream::connect(addr).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    stream.write_all(raw.as_bytes()).unwrap();
    let mut buf = Vec::new();
    let _ = stream.read_to_end(&mut buf);
    String::from_utf8_lossy(&buf).into_owned()
}

pub(super) fn form_post(body: &str) -> String {
    format!(
        "POST /login HTTP/1.1\r\nHost: x\r\n\
         Content-Type: application/x-www-form-urlencoded\r\n\
         Content-Length: {}\r\nConnection: close\r\n\r\n{}",
        body.len(),
        body
    )
}

pub(super) fn session_token(response: &str) -> Option<String> {
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
