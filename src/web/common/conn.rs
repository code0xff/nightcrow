//! Connection handling shared by both web servers: accept-loop accounting,
//! reading a request off a socket, and the cross-site origin check.
//!
//! Each live connection costs a handler thread, so a server that accepts
//! without a bound lets anything reaching the port exhaust the process. The
//! cap itself is each server's policy; this module only hands out and reclaims
//! the slots.
//!
//! The request reader and [`origin_allowed`] live here rather than in either
//! server so the two cannot drift apart — a second, looser spelling of a
//! security check is how a bypass gets in.

use crate::web::common::http::{self, RequestHead};
use anyhow::{Context, Result};
use std::io::Read;
use std::net::TcpStream;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

/// Reject a request head larger than this (headers only) to bound memory.
pub const MAX_HEAD_BYTES: usize = 32 * 1024;
/// Cap the request body read. Both servers take only small form posts.
pub const MAX_BODY_BYTES: usize = 64 * 1024;
/// Per-read socket timeout while collecting the head.
pub const HEAD_READ_TIMEOUT: Duration = Duration::from_secs(15);
/// Wall-clock budget for the *whole* request. The socket timeout above only
/// bounds one `read` and re-arms on every byte — a client dribbling one byte
/// per timeout would otherwise hold a connection slot for days.
pub const REQUEST_DEADLINE: Duration = Duration::from_secs(30);

/// Read the request head (up to CRLFCRLF) plus any declared body. Both
/// ceilings are enforced while reading, not after, so a client that never
/// sends a terminator cannot grow the buffer without bound.
pub fn read_request(stream: &mut TcpStream) -> Result<(RequestHead, String)> {
    stream.set_read_timeout(Some(HEAD_READ_TIMEOUT)).ok();
    let deadline = Instant::now() + REQUEST_DEADLINE;
    let mut buf = Vec::new();
    let mut chunk = [0u8; 4096];
    let head_end = loop {
        if let Some(head_end) = request_head_end(&buf)? {
            break head_end;
        }
        if Instant::now() >= deadline {
            anyhow::bail!("request head did not arrive within {REQUEST_DEADLINE:?}");
        }
        let n = stream.read(&mut chunk).context("reading request head")?;
        if n == 0 {
            anyhow::bail!("connection closed before the request head completed");
        }
        buf.extend_from_slice(&chunk[..n]);
    };

    let head_text =
        std::str::from_utf8(&buf[..head_end]).context("request head is not valid UTF-8")?;
    let head = http::parse_request_head(head_text)?;

    let want = head.content_length.min(MAX_BODY_BYTES);
    let mut body = buf[head_end..].to_vec();
    while body.len() < want {
        if Instant::now() >= deadline {
            anyhow::bail!("request body did not arrive within {REQUEST_DEADLINE:?}");
        }
        let n = stream.read(&mut chunk).context("reading request body")?;
        if n == 0 {
            break;
        }
        body.extend_from_slice(&chunk[..n]);
    }
    body.truncate(want);
    // A WebSocket loop installs its own timeout; clear this one first.
    stream.set_read_timeout(None).ok();
    Ok((head, String::from_utf8_lossy(&body).into_owned()))
}

fn request_head_end(buf: &[u8]) -> Result<Option<usize>> {
    if let Some(pos) = find_subsequence(buf, b"\r\n\r\n") {
        let head_end = pos + 4;
        if head_end > MAX_HEAD_BYTES {
            anyhow::bail!("request head exceeds {MAX_HEAD_BYTES} bytes");
        }
        return Ok(Some(head_end));
    }
    if buf.len() >= MAX_HEAD_BYTES {
        anyhow::bail!("request head exceeds {MAX_HEAD_BYTES} bytes");
    }
    Ok(None)
}

/// Whether a request's `Origin` is acceptable.
///
/// An absent Origin (a native client) is allowed; a present one must match the
/// request `Host` authority, else it is a cross-site request and is refused.
pub fn origin_allowed(head: &RequestHead) -> bool {
    match head.header("origin") {
        None => true,
        Some(origin) => {
            let origin_authority = origin.split_once("://").map(|(_, rest)| rest);
            matches!(
                (origin_authority, head.header("host")),
                (Some(authority), Some(host)) if authority == host
            )
        }
    }
}

/// Whether the request's `Host` names an address this server should answer on.
///
/// [`origin_allowed`] only proves Origin and Host *agree*, which a
/// DNS-rebound attacker satisfies trivially: they control both. Rebinding
/// `evil.example` to 127.0.0.1 would otherwise give their page a same-origin
/// position from which to POST `/login` and read the reply.
///
/// A loopback-bound server can only legitimately be addressed as loopback, so
/// any other Host is refused. Bound off-loopback, the operator has taken
/// responsibility for the network path, and the check would reject legitimate
/// proxied hosts, so it does not apply.
pub fn host_allowed(head: &RequestHead, bound_loopback: bool) -> bool {
    if !bound_loopback {
        return true;
    }
    let Some(host) = head.header("host") else {
        // HTTP/1.1 requires Host; a request without one is not from a browser
        // and cannot be a rebinding victim.
        return true;
    };
    // Strip the port, and the brackets of an IPv6 literal.
    let name = match host.rsplit_once(':') {
        Some((name, port)) if port.chars().all(|c| c.is_ascii_digit()) => name,
        _ => host,
    };
    let name = name.trim_start_matches('[').trim_end_matches(']');
    name.eq_ignore_ascii_case("localhost")
        || name == "127.0.0.1"
        || name == "::1"
        || name
            .parse::<std::net::IpAddr>()
            .is_ok_and(|ip| ip.is_loopback())
}

pub fn find_subsequence(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

/// A claimed connection slot. Releasing it is `Drop`, so every handler exit
/// path — normal return, early error, a panicking thread — frees the slot.
pub struct ConnectionSlot {
    counter: Arc<AtomicUsize>,
}

impl ConnectionSlot {
    /// Claim a slot, or return `None` when `counter` is already at `cap`.
    pub fn acquire(counter: &Arc<AtomicUsize>, cap: usize) -> Option<Self> {
        // Claim first and give back on overflow, so two accepts racing at the
        // limit cannot both see room and both proceed.
        let previous = counter.fetch_add(1, Ordering::AcqRel);
        if previous >= cap {
            counter.fetch_sub(1, Ordering::AcqRel);
            return None;
        }
        Some(Self {
            counter: Arc::clone(counter),
        })
    }
}

impl Drop for ConnectionSlot {
    fn drop(&mut self) {
        self.counter.fetch_sub(1, Ordering::AcqRel);
    }
}

/// Largest WebSocket message either server will accept. Terminal input is
/// keystrokes and pastes; the mirror's is small JSON.
pub const MAX_WS_MESSAGE_BYTES: usize = 1024 * 1024;

/// Complete a WebSocket upgrade on `stream`, or answer the error and give up.
///
/// Shared by both servers: the handshake needs only the request head and the
/// socket, so the mirror and the viewer differ only in what they do with the
/// resulting connection.
pub fn websocket_handshake(
    mut stream: TcpStream,
    head: &RequestHead,
) -> Option<tungstenite::WebSocket<TcpStream>> {
    use std::io::Write;

    let Some(key) = head.header("sec-websocket-key") else {
        let _ = stream.write_all(&http::response(
            "400 Bad Request",
            "text/plain; charset=utf-8",
            &[],
            b"missing Sec-WebSocket-Key",
        ));
        return None;
    };
    let accept = tungstenite::handshake::derive_accept_key(key.as_bytes());
    let handshake = format!(
        "HTTP/1.1 101 Switching Protocols\r\n\
         Upgrade: websocket\r\n\
         Connection: Upgrade\r\n\
         Sec-WebSocket-Accept: {accept}\r\n\r\n"
    );
    if stream.write_all(handshake.as_bytes()).is_err() {
        return None;
    }
    // Cap frame and message size: tungstenite's defaults (16 MiB / 64 MiB)
    // paired with the terminal command queue could park gigabytes of pending
    // input. Nothing either server accepts is large.
    let config = tungstenite::protocol::WebSocketConfig::default()
        .max_message_size(Some(MAX_WS_MESSAGE_BYTES))
        .max_frame_size(Some(MAX_WS_MESSAGE_BYTES));
    Some(tungstenite::WebSocket::from_raw_socket(
        stream,
        tungstenite::protocol::Role::Server,
        Some(config),
    ))
}

#[cfg(test)]
mod tests;
