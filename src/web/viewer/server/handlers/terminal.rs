//! Accepting one authenticated terminal socket and handing it to its threads.

use super::super::mutations::lookup_repo;
use super::super::{SSE_HEARTBEAT, ViewerState};
use super::terminal_connection::run;
use super::terminal_liveness::Timing;
use super::terminal_pipe::{Duplex, Link, READ_GUARD};
use crate::session::size_owner::ViewerId;
use crate::web::common::conn;
use std::io::Write;
use std::net::TcpStream;
use std::sync::Arc;

/// The longest `viewer` id accepted. Long enough for a UUID with room to spare.
const MAX_VIEWER_ID: usize = 64;

/// A page's identity for the session's size ownership, from what it called
/// itself.
///
/// The page generates this once per tab and sends it on every socket, so its
/// connections can come and go without the session reading them as somebody
/// new sitting down. A missing or malformed id gets one of its own rather
/// than a refusal — the page still works, it simply behaves as it did before
/// it could name itself.
fn browser_viewer(head: &crate::web::common::http::RequestHead) -> ViewerId {
    let named = head.query_param("viewer").filter(|id| {
        !id.is_empty()
            && id.len() <= MAX_VIEWER_ID
            && id
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    });
    match named {
        Some(id) => ViewerId::Browser(id),
        None => {
            tracing::debug!("viewer: a terminal socket did not name its page");
            ViewerId::Browser(anonymous_viewer())
        }
    }
}

fn anonymous_viewer() -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(0);
    format!("conn-{}", NEXT.fetch_add(1, Ordering::Relaxed))
}

/// Hand this connection to the repository's terminal hub.
///
/// Auth and Origin were already enforced by `handle_connection`, before the
/// repository was named -- a terminal is effectively a shell, so the upgrade
/// must never be reachable ahead of those checks.
pub(in crate::web::viewer::server) fn serve_terminal(
    stream: TcpStream,
    head: &crate::web::common::http::RequestHead,
    state: &ViewerState,
) {
    let mut stream = stream;
    let entry = match lookup_repo(head, state) {
        Ok(entry) => entry,
        Err(response) => {
            let _ = stream.write_all(&response);
            return;
        }
    };
    // A write to a page that stopped reading must not park this thread forever.
    // The read half's own timeout is set with its thread, where it doubles as
    // the exit guard.
    let _ = stream.set_write_timeout(Some(SSE_HEARTBEAT));
    // Terminal frames are small and latency matters more than packing them:
    // Nagle would hold a keystroke's echo until an unrelated write completed.
    let _ = stream.set_nodelay(true);
    // Taken before the stream is moved into the websocket, which owns it from
    // there on. Two clones: one the hub keeps to cut a stalled client off, one
    // for the reader thread that blocks on this connection's input.
    let evict_handle = stream.try_clone();
    let read_half = match stream.try_clone() {
        Ok(socket) => socket,
        Err(err) => {
            // Without a second handle nothing can read the page's input, so the
            // connection would look alive and never answer.
            tracing::warn!(%err, "viewer: could not split a terminal socket; refusing it");
            return;
        }
    };
    let Some(ws) = conn::websocket_handshake(Duplex::new(stream), head) else {
        return;
    };
    // `claim` is the page saying a person just opened it, as opposed to a
    // repository switch or a reconnect. Absent means no — a socket that does
    // not say it arrived must not take the sizing off whoever is looking.
    let arriving = head.query_param("claim").as_deref() == Some("1");
    // A clone, kept by the hub only to end this connection if the page stops
    // draining its queue. A clone that could not be made costs the hub that
    // ability and nothing else.
    let evict_handle = match evict_handle {
        Ok(handle) => Some(handle),
        Err(err) => {
            // Degrades to no handle: the client is dropped from the broadcast
            // list but its socket stays open. Logged because the page then
            // holds a panel that has stopped updating and nothing else says
            // so — and a failed clone means descriptors are exhausted.
            tracing::warn!(%err, "viewer: a terminal socket cannot be cut off if it stalls");
            None
        }
    };
    let session = Arc::new(
        entry
            .terminals
            .connect(browser_viewer(head), arriving, evict_handle),
    );
    // Paired with the ending line the connection loop logs: between the two the
    // log holds this connection's whole span, and the repo names which panel it
    // was — a page cycling sockets shows here as rapid attach lines for one
    // viewer.
    tracing::info!(
        repo = head.query_param("repo").as_deref().unwrap_or("?"),
        viewer = ?browser_viewer(head),
        arriving,
        "viewer: terminal socket attached"
    );
    let link = match Link::open(read_half, &session, READ_GUARD) {
        Ok(link) => link,
        Err(err) => {
            tracing::warn!(%err, "viewer: could not start the threads behind a terminal socket");
            return;
        }
    };
    run(ws, link, session, Timing::default());
}

#[cfg(test)]
#[path = "terminal_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "terminal_fixture.rs"]
mod fixture;

#[cfg(test)]
#[path = "terminal_socket_tests.rs"]
mod socket_tests;
