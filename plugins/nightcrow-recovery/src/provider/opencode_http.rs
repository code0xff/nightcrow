//! The OpenCode server's wire side: how a status snapshot is fetched, and how
//! the payload is read once it arrives.
//!
//! Split out of `opencode.rs` so the adapter's state machine reads as state
//! transitions only. Everything here is either a pure function over bytes or a
//! single loopback request, and nothing here decides anything about a pane.

use crate::provider::{MAX_RESET_HORIZON_SECS, plausible_reset};
use anyhow::Context;
use serde_json::Value;
use std::io::{Read, Write};
use std::net::{Ipv4Addr, SocketAddr, TcpStream};
use std::time::Duration;

/// Ceiling on one whole response, headers included. A status snapshot is a few
/// KiB; the cap exists so a wedged or hostile socket cannot make this process
/// allocate without bound.
const MAX_RESPONSE_BYTES: usize = 256 * 1024;

/// Boundary between an HTTP head and its body.
const HEAD_BODY_SEPARATOR: &str = "\r\n\r\n";

/// The only status code whose body is worth parsing.
const OK_STATUS: &str = "200";

/// Divisor between the millisecond and second readings of an ambiguous `next`.
const MILLIS_PER_SEC: i64 = 1_000;

/// Longest `next` accepted as a *relative* delay, in milliseconds — OpenCode
/// states its own backoff in milliseconds. Bounded by the same horizon the rest
/// of the plugin trusts, so a relative delay cannot park a pane for longer than
/// an absolute reset time could.
const MAX_RELATIVE_NEXT_MILLIS: i64 = MAX_RESET_HORIZON_SECS * MILLIS_PER_SEC;

/// Keys an entry may carry its session id under. The envelope's schema is
/// unverified, so every plausible spelling is accepted.
const SESSION_ID_KEYS: &[&str] = &["sessionID", "sessionId", "id"];

/// Where a status snapshot comes from. Exists so the state logic is testable
/// without a listening socket.
pub trait StatusSource: std::fmt::Debug {
    fn fetch(&mut self) -> anyhow::Result<String>;
}

/// One session's status, as far as we could understand it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionStatus {
    pub session_id: Option<String>,
    pub kind: StatusKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatusKind {
    Retry {
        attempt: u32,
        next: Option<i64>,
    },
    Busy,
    Idle,
    /// A `type` we do not model. Kept rather than dropped so a future status
    /// value reads as "not a retry" instead of as "no session here".
    Unknown,
}

/// Read a status snapshot, tolerating both envelope shapes seen in the wild: an
/// object keyed by session id, and an array of per-session entries.
///
/// Never errors. Malformed JSON, a bare scalar, or an entry shaped a third way
/// yields no statuses, because one unreadable poll must not fail the plugin.
pub fn parse_status_body(body: &str) -> Vec<SessionStatus> {
    let Ok(root) = serde_json::from_str::<Value>(body) else {
        return Vec::new();
    };
    match &root {
        Value::Object(map) => map
            .iter()
            .filter_map(|(key, entry)| read_entry(entry, Some(key.as_str())))
            .collect(),
        Value::Array(items) => items.iter().filter_map(|e| read_entry(e, None)).collect(),
        _ => Vec::new(),
    }
}

/// Turn one entry into a status, or `None` when it holds no status object.
fn read_entry(entry: &Value, key: Option<&str>) -> Option<SessionStatus> {
    let status = status_object(entry)?;
    let session_id = SESSION_ID_KEYS
        .iter()
        .find_map(|k| entry.get(k).and_then(Value::as_str))
        .or(key)
        .map(str::to_string);
    Some(SessionStatus {
        session_id,
        kind: status_kind(status),
    })
}

/// The entry either *is* the status object or wraps one under `status`.
fn status_object(entry: &Value) -> Option<&Value> {
    if entry.get("type").is_some() {
        return Some(entry);
    }
    let nested = entry.get("status")?;
    nested.get("type").map(|_| nested)
}

fn status_kind(status: &Value) -> StatusKind {
    match status.get("type").and_then(Value::as_str) {
        Some("retry") => StatusKind::Retry {
            // Informational only — nothing is decided from the attempt number —
            // so a missing one is not a parse failure.
            attempt: status
                .get("attempt")
                .and_then(Value::as_u64)
                .and_then(|n| u32::try_from(n).ok())
                .unwrap_or(0),
            next: status.get("next").and_then(Value::as_i64),
        },
        Some("busy") => StatusKind::Busy,
        Some("idle") => StatusKind::Idle,
        _ => StatusKind::Unknown,
    }
}

/// Resolve the ambiguous `next` field to an absolute unix time in **seconds**.
///
/// Whether OpenCode reports an absolute epoch (in which unit) or a relative
/// delay is unverified, so all three readings are tried. The order is by safety
/// rather than by likelihood: absolute readings come first, because over-waiting
/// only costs time while firing early walks straight back into the limit. `None`
/// means "no deadline", which degrades to the machine's own bounded backoff.
pub fn interpret_next(next: i64, now_epoch: i64) -> Option<i64> {
    // Zero or negative is "now" or a corrupt value; both would fire immediately,
    // so neither is accepted as a deadline.
    if next <= 0 {
        return None;
    }
    if let Some(at) = plausible_reset(next, now_epoch) {
        return Some(at);
    }
    if let Some(at) = plausible_reset(next / MILLIS_PER_SEC, now_epoch) {
        return Some(at);
    }
    if next <= MAX_RELATIVE_NEXT_MILLIS {
        return Some(now_epoch.saturating_add(next / MILLIS_PER_SEC));
    }
    None
}

/// `GET` one loopback path and return the response body.
///
/// Loopback only, by construction: the host is not a parameter, so this can
/// never be pointed at a remote server. `timeout` bounds the connect, the write
/// and the read separately, so no single stage can hang the plugin's loop.
///
/// Deliberately no transfer-encoding handling: a chunked answer comes back with
/// its framing intact, [`parse_status_body`] then finds no statuses in it, and
/// the poll degrades to "nothing to report" — the same outcome as no server at
/// all. That is the right failure for an adapter that must never guess.
pub fn http_get(port: u16, path: &str, timeout: Duration) -> anyhow::Result<String> {
    anyhow::ensure!(
        is_safe_path(path),
        "refusing opencode request path {path:?}: only /, -, _ and ASCII alphanumerics may reach \
         the request line"
    );
    let addr = SocketAddr::from((Ipv4Addr::LOCALHOST, port));
    let mut stream = TcpStream::connect_timeout(&addr, timeout)
        .map_err(|e| anyhow::anyhow!("cannot reach the opencode server at {addr}: {e}"))?;
    stream
        .set_read_timeout(Some(timeout))
        .and_then(|()| stream.set_write_timeout(Some(timeout)))
        .context("cannot bound the opencode socket's read and write time")?;
    let request = format!(
        "GET {path} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nAccept: application/json\r\n\
         Connection: close\r\n\r\n"
    );
    stream
        .write_all(request.as_bytes())
        .and_then(|()| stream.flush())
        .with_context(|| format!("cannot send GET {path} to the opencode server"))?;
    let raw = read_capped(&mut stream).with_context(|| format!("reading GET {path}"))?;
    let text = String::from_utf8(raw)
        .map_err(|_| anyhow::anyhow!("opencode server answered with non-UTF-8 bytes"))?;
    let (head, body) = text
        .split_once(HEAD_BODY_SEPARATOR)
        .ok_or_else(|| anyhow::anyhow!("opencode server answer has no header/body boundary"))?;
    let code = head
        .lines()
        .next()
        .unwrap_or_default()
        .split_whitespace()
        .nth(1)
        .unwrap_or_default();
    anyhow::ensure!(
        code == OK_STATUS,
        "opencode server answered status {code:?}, not {OK_STATUS}"
    );
    Ok(body.to_string())
}

/// Read at most [`MAX_RESPONSE_BYTES`], and refuse rather than truncate.
///
/// Reads one byte past the cap so an over-long answer is *detected* — silently
/// truncating would hand the parser a body that only looks malformed.
fn read_capped(source: impl Read) -> anyhow::Result<Vec<u8>> {
    let mut buf = Vec::new();
    let mut limited = source.take(MAX_RESPONSE_BYTES as u64 + 1);
    limited.read_to_end(&mut buf)?;
    anyhow::ensure!(
        buf.len() <= MAX_RESPONSE_BYTES,
        "opencode server answer exceeds the {MAX_RESPONSE_BYTES}-byte cap"
    );
    Ok(buf)
}

/// An absolute path made only of characters that cannot alter the request line.
/// Whitespace, CR and LF are the ones that matter: any of them would let a
/// caller append headers or a second request.
fn is_safe_path(path: &str) -> bool {
    path.starts_with('/')
        && path
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '/' | '-' | '_'))
}

#[cfg(test)]
#[path = "opencode_http_tests.rs"]
mod tests;
