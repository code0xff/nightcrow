//! Minimal synchronous HTTP/1.1 request parsing and response building for the
//! web mirror. Only the tiny surface the server needs is implemented (a static
//! page, a login POST, a WebSocket upgrade) — no HTTP crate is pulled in.
//!
//! The parsing here is pure and unit-tested; the socket I/O that feeds it lives
//! in `server.rs`.

use anyhow::{Result, bail};

/// The parsed head (request line + headers) of an HTTP request. Header names
/// are lowercased for case-insensitive lookup.
#[derive(Debug, Clone)]
pub struct RequestHead {
    pub method: String,
    /// Path with the query string stripped (e.g. `/login`).
    pub path: String,
    pub headers: Vec<(String, String)>,
    /// Declared body length from `Content-Length`, 0 when absent/invalid.
    pub content_length: usize,
}

impl RequestHead {
    pub fn header(&self, name: &str) -> Option<&str> {
        let name = name.to_ascii_lowercase();
        self.headers
            .iter()
            .find(|(k, _)| *k == name)
            .map(|(_, v)| v.as_str())
    }

    /// Look up a cookie by name from the `Cookie` header.
    pub fn cookie(&self, name: &str) -> Option<&str> {
        let raw = self.header("cookie")?;
        for pair in raw.split(';') {
            let pair = pair.trim();
            if let Some((k, v)) = pair.split_once('=')
                && k.trim() == name
            {
                return Some(v.trim());
            }
        }
        None
    }

    /// True when the request is a WebSocket upgrade handshake.
    pub fn is_websocket_upgrade(&self) -> bool {
        let upgrade = self
            .header("upgrade")
            .is_some_and(|v| v.eq_ignore_ascii_case("websocket"));
        let connection = self
            .header("connection")
            .is_some_and(|v| v.to_ascii_lowercase().contains("upgrade"));
        upgrade && connection
    }
}

/// Parse the request head (everything up to, but not including, the body).
pub fn parse_request_head(text: &str) -> Result<RequestHead> {
    let mut lines = text.split("\r\n");
    let request_line = lines.next().unwrap_or_default();
    let mut parts = request_line.split_whitespace();
    let method = parts.next().unwrap_or_default().to_string();
    let target = parts.next().unwrap_or_default().to_string();
    if method.is_empty() || target.is_empty() {
        bail!("malformed HTTP request line: {request_line:?}");
    }
    // Strip any query string so routing matches on the path alone; the query
    // itself is not used by the current routes.
    let path = match target.split_once('?') {
        Some((p, _)) => p.to_string(),
        None => target,
    };

    let mut headers = Vec::new();
    let mut content_length = 0;
    for line in lines {
        if line.is_empty() {
            break;
        }
        if let Some((name, value)) = line.split_once(':') {
            let name = name.trim().to_ascii_lowercase();
            let value = value.trim().to_string();
            if name == "content-length" {
                content_length = value.parse().unwrap_or(0);
            }
            headers.push((name, value));
        }
    }

    Ok(RequestHead {
        method,
        path,
        headers,
        content_length,
    })
}

/// Parse an `application/x-www-form-urlencoded` body into key/value pairs.
pub fn parse_form(body: &str) -> Vec<(String, String)> {
    body.split('&')
        .filter_map(|pair| {
            let (k, v) = pair.split_once('=')?;
            Some((percent_decode(k), percent_decode(v)))
        })
        .collect()
}

/// Look up a field in a parsed form body.
pub fn form_field<'a>(fields: &'a [(String, String)], key: &str) -> Option<&'a str> {
    fields
        .iter()
        .find(|(k, _)| k == key)
        .map(|(_, v)| v.as_str())
}

/// Decode `application/x-www-form-urlencoded` / query text: `+` → space and
/// `%XX` → byte. Invalid escapes are passed through literally.
fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'+' => {
                out.push(b' ');
                i += 1;
            }
            b'%' if i + 2 < bytes.len() => {
                let hi = (bytes[i + 1] as char).to_digit(16);
                let lo = (bytes[i + 2] as char).to_digit(16);
                if let (Some(hi), Some(lo)) = (hi, lo) {
                    out.push((hi * 16 + lo) as u8);
                    i += 3;
                } else {
                    out.push(bytes[i]);
                    i += 1;
                }
            }
            b => {
                out.push(b);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Build a raw HTTP/1.1 response.
pub fn response(
    status: &str,
    content_type: &str,
    extra_headers: &[(&str, &str)],
    body: &[u8],
) -> Vec<u8> {
    let mut out = Vec::with_capacity(body.len() + 128);
    out.extend_from_slice(format!("HTTP/1.1 {status}\r\n").as_bytes());
    out.extend_from_slice(format!("Content-Type: {content_type}\r\n").as_bytes());
    out.extend_from_slice(format!("Content-Length: {}\r\n", body.len()).as_bytes());
    // These pages carry a live session; never let a cache retain them.
    out.extend_from_slice(b"Cache-Control: no-store\r\n");
    for (name, value) in extra_headers {
        out.extend_from_slice(format!("{name}: {value}\r\n").as_bytes());
    }
    out.extend_from_slice(b"Connection: close\r\n\r\n");
    out.extend_from_slice(body);
    out
}

pub fn html(status: &str, body: &str) -> Vec<u8> {
    response(status, "text/html; charset=utf-8", &[], body.as_bytes())
}

pub fn redirect(location: &str, extra_headers: &[(&str, &str)]) -> Vec<u8> {
    let mut headers = vec![("Location", location)];
    headers.extend_from_slice(extra_headers);
    response("303 See Other", "text/plain; charset=utf-8", &headers, b"")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(text: &str) -> RequestHead {
        parse_request_head(text).unwrap()
    }

    #[test]
    fn parses_method_and_strips_query_from_path() {
        let head = parse("GET /login?token=abc&x=1 HTTP/1.1\r\nHost: localhost\r\n\r\n");
        assert_eq!(head.method, "GET");
        assert_eq!(
            head.path, "/login",
            "the query string must be stripped for routing"
        );
    }

    #[test]
    fn header_lookup_is_case_insensitive() {
        let head = parse("GET / HTTP/1.1\r\nContent-Length: 5\r\nX-Foo: Bar\r\n\r\n");
        assert_eq!(head.header("content-length"), Some("5"));
        assert_eq!(head.header("X-FOO"), Some("Bar"));
        assert_eq!(head.content_length, 5);
    }

    #[test]
    fn parses_cookies() {
        let head = parse("GET / HTTP/1.1\r\nCookie: a=1; nightcrow_session=tok123; b=2\r\n\r\n");
        assert_eq!(head.cookie("nightcrow_session"), Some("tok123"));
        assert_eq!(head.cookie("a"), Some("1"));
        assert_eq!(head.cookie("missing"), None);
    }

    #[test]
    fn detects_websocket_upgrade() {
        let ws = parse("GET /ws HTTP/1.1\r\nUpgrade: websocket\r\nConnection: Upgrade\r\n\r\n");
        assert!(ws.is_websocket_upgrade());
        let plain = parse("GET / HTTP/1.1\r\nConnection: keep-alive\r\n\r\n");
        assert!(!plain.is_websocket_upgrade());
    }

    #[test]
    fn detects_websocket_upgrade_with_combined_connection_header() {
        // Browsers often send "Connection: keep-alive, Upgrade".
        let ws = parse(
            "GET /ws HTTP/1.1\r\nUpgrade: WebSocket\r\nConnection: keep-alive, Upgrade\r\n\r\n",
        );
        assert!(ws.is_websocket_upgrade());
    }

    #[test]
    fn rejects_malformed_request_line() {
        assert!(parse_request_head("garbage\r\n\r\n").is_err());
        assert!(parse_request_head("").is_err());
    }

    #[test]
    fn parses_url_encoded_form_body() {
        let fields = parse_form("password=p%40ss+word&remember=on");
        assert_eq!(form_field(&fields, "password"), Some("p@ss word"));
        assert_eq!(form_field(&fields, "remember"), Some("on"));
        assert_eq!(form_field(&fields, "missing"), None);
    }

    #[test]
    fn response_has_content_length_and_no_store() {
        let bytes = response("200 OK", "text/plain", &[("X-A", "b")], b"hello");
        let text = String::from_utf8_lossy(&bytes);
        assert!(text.starts_with("HTTP/1.1 200 OK\r\n"));
        assert!(text.contains("Content-Length: 5\r\n"));
        assert!(text.contains("Cache-Control: no-store\r\n"));
        assert!(text.contains("X-A: b\r\n"));
        assert!(text.ends_with("\r\n\r\nhello"));
    }

    #[test]
    fn redirect_carries_location_and_extra_headers() {
        let bytes = redirect("/", &[("Set-Cookie", "s=1")]);
        let text = String::from_utf8_lossy(&bytes);
        assert!(text.starts_with("HTTP/1.1 303 See Other\r\n"));
        assert!(text.contains("Location: /\r\n"));
        assert!(text.contains("Set-Cookie: s=1\r\n"));
    }
}
