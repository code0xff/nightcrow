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
use std::time::Duration;

/// Reject a request head larger than this (headers only) to bound memory.
pub const MAX_HEAD_BYTES: usize = 32 * 1024;
/// Cap the request body read. Both servers take only small form posts.
pub const MAX_BODY_BYTES: usize = 64 * 1024;
/// Give a client this long to send its request head before dropping it.
pub const HEAD_READ_TIMEOUT: Duration = Duration::from_secs(15);

/// Read the request head (up to CRLFCRLF) plus any declared body.
///
/// Both ceilings are enforced while reading, not after, so a client that never
/// sends a terminator cannot grow the buffer without bound.
pub fn read_request(stream: &mut TcpStream) -> Result<(RequestHead, String)> {
    stream.set_read_timeout(Some(HEAD_READ_TIMEOUT)).ok();
    let mut buf = Vec::new();
    let mut chunk = [0u8; 4096];
    let head_end = loop {
        if let Some(pos) = find_subsequence(&buf, b"\r\n\r\n") {
            break pos + 4;
        }
        if buf.len() > MAX_HEAD_BYTES {
            anyhow::bail!("request head exceeds {MAX_HEAD_BYTES} bytes");
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

/// Whether a request's `Origin` is acceptable.
///
/// An absent Origin (a native client, which cannot carry a browser's cookie)
/// is allowed; a present one must match the request `Host` authority, else it
/// is a cross-site request and is refused. `SameSite=Strict` already keeps the
/// session cookie off cross-site requests, so a hijack fails auth anyway —
/// this refuses it outright.
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn connection_slot_refuses_over_the_cap() {
        let counter = Arc::new(AtomicUsize::new(0));

        let held: Vec<_> = (0..2)
            .map(|_| ConnectionSlot::acquire(&counter, 2).expect("under the cap"))
            .collect();

        assert!(
            ConnectionSlot::acquire(&counter, 2).is_none(),
            "a third connection must be refused"
        );
        assert_eq!(
            counter.load(Ordering::Acquire),
            2,
            "a refused connection must not leak a slot"
        );
        drop(held);
    }

    #[test]
    fn connection_slot_releases_on_drop() {
        let counter = Arc::new(AtomicUsize::new(0));

        drop(ConnectionSlot::acquire(&counter, 1).expect("under the cap"));

        assert_eq!(counter.load(Ordering::Acquire), 0);
        assert!(
            ConnectionSlot::acquire(&counter, 1).is_some(),
            "the freed slot must be reusable"
        );
    }
}
