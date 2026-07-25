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
    /// Raw query string without the `?` (e.g. `repo=a&path=src`), empty when
    /// the target carried none. Read it through [`RequestHead::query_param`]
    /// rather than parsing it again at each call site.
    ///
    /// The mirror's routes take no parameters; this exists for the viewer's
    /// `?repo=&path=` routes (step 6 of `docs/web-viewer-plan.md`).
    #[allow(dead_code)]
    pub query: String,
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

    /// Look up a query parameter, percent-decoded. Returns the first match when
    /// a name repeats, so a duplicate cannot be used to smuggle a second value
    /// past a check that read the first.
    ///
    /// The result is decoded exactly once. Callers that turn a parameter into a
    /// filesystem path must validate *this* value — decoding again afterwards
    /// would let `%252e%252e` become `..` after the check.
    #[allow(dead_code)] // First caller is the viewer's git routes; see `query`.
    pub fn query_param(&self, name: &str) -> Option<String> {
        parse_form(&self.query)
            .into_iter()
            .find(|(k, _)| k == name)
            .map(|(_, v)| v)
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
    // Split the query off so routing matches on the path alone.
    let (path, query) = match target.split_once('?') {
        Some((p, q)) => (p.to_string(), q.to_string()),
        None => (target, String::new()),
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
        query,
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
#[path = "http_tests.rs"]
mod tests;
