use super::super::mutations::lookup_repo;
use super::super::{SSE_HEARTBEAT, TERM_POLL_TIMEOUT, ViewerState};
use crate::session::size_owner::ViewerId;
use crate::session::terminal::{self, ClientMessage, TerminalFrame};
use crate::web::common::conn;
use std::io::Write;
use std::net::TcpStream;
use std::time::Duration;
use tungstenite::Message;

/// The longest `viewer` id accepted. Long enough for a UUID with room to spare.
const MAX_VIEWER_ID: usize = 64;

/// A page's identity for the session's size ownership, from what it called
/// itself.
///
/// The page generates this once per tab and sends it on every socket, so its
/// connections can come and go without the session reading them as somebody new
/// sitting down.
///
/// A boundary input, so it is held to what an id can be: a short run of plain
/// characters. An id that is missing or malformed gets one of its own rather
/// than a refusal -- the page still works, it simply behaves as it did before it
/// could name itself.
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
    // Without a read timeout, `ws.read()` blocks and terminal output would
    // only flush when the user happened to type. The timeout turns the loop
    // into a poll that services both directions.
    let _ = stream.set_read_timeout(Some(TERM_POLL_TIMEOUT));
    let _ = stream.set_write_timeout(Some(SSE_HEARTBEAT));
    // Taken before the stream is moved into the websocket, which owns it from
    // there on.
    let evict_handle = stream.try_clone();
    let Some(mut ws) = conn::websocket_handshake(stream, head) else {
        return;
    };
    // `claim` is the page saying a person just opened it, as opposed to a
    // repository switch or a reconnect. Absent means no -- a socket that does not
    // say it arrived must not take the sizing off whoever is looking.
    let arriving = head.query_param("claim").as_deref() == Some("1");
    // A second handle, kept by the hub only to end this connection if the page
    // stops draining its queue: the loop below is then parked in `ws.read()` and
    // nothing else would wake it. A clone that could not be made costs the hub
    // that ability and nothing else.
    let evict_handle = match evict_handle {
        Ok(handle) => Some(handle),
        Err(err) => {
            // Degrades to what this did before there was a handle at all: the
            // client is dropped from the broadcast list but its socket stays
            // open. Logged rather than passed over, because the page then holds
            // a panel that has stopped updating and nothing else says so -- and
            // because a clone that fails means descriptors are exhausted, which
            // is worth knowing on its own.
            tracing::warn!(%err, "viewer: a terminal socket cannot be cut off if it stalls");
            None
        }
    };
    let session = entry
        .terminals
        .connect(browser_viewer(head), arriving, evict_handle);

    loop {
        // Drain everything queued for us before blocking on the socket, so
        // output is not held back waiting for the client to say something.
        let mut wrote = false;
        while let Some(frame) = session.next_frame(Duration::from_millis(1)) {
            let message = match frame {
                TerminalFrame::Output { pane, data } => {
                    Message::Binary(terminal::encode_output(pane, &data).into())
                }
                TerminalFrame::Control(json) => Message::Text(json.into()),
            };
            if ws.send(message).is_err() {
                return;
            }
            wrote = true;
        }
        if wrote && ws.flush().is_err() {
            return;
        }

        match ws.read() {
            Ok(Message::Text(text)) => match serde_json::from_str::<ClientMessage>(&text) {
                Ok(message) => session.dispatch(message),
                // A malformed frame is dropped, not fatal: a client bug should
                // not take the terminal down with it.
                Err(err) => tracing::debug!(%err, "viewer: bad terminal message"),
            },
            Ok(Message::Close(_)) => return,
            Ok(_) => {}
            // A poll timeout surfaces as WouldBlock on macOS and TimedOut on
            // Linux; neither means the client is gone.
            Err(tungstenite::Error::Io(err))
                if matches!(
                    err.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                ) => {}
            Err(_) => return,
        }
    }
    // `session` drops here, unregistering from the hub.
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::web::common::http::RequestHead;

    fn head(query: &str) -> RequestHead {
        crate::web::common::http::parse_request_head(&format!(
            "GET /ws/term?{query} HTTP/1.1\r\nHost: h\r\n\r\n"
        ))
        .expect("a well-formed request line")
    }

    #[test]
    fn a_page_that_names_itself_is_taken_at_its_word() {
        assert_eq!(
            browser_viewer(&head("repo=r1&viewer=tab-9f2c")),
            ViewerId::Browser("tab-9f2c".to_string())
        );
    }

    /// The whole point of the id is that the same page keeps it across sockets.
    #[test]
    fn the_same_page_is_the_same_viewer_on_every_socket() {
        assert_eq!(
            browser_viewer(&head("repo=r1&viewer=tab-1")),
            browser_viewer(&head("repo=r2&viewer=tab-1")),
        );
    }

    /// A stale bundle, or something that is not a page at all. It still works --
    /// it just behaves as a browser did before it could name itself.
    #[test]
    fn a_socket_with_no_usable_id_gets_one_of_its_own() {
        let long = "a".repeat(MAX_VIEWER_ID + 1);
        for query in [
            "repo=r1".to_string(),
            "repo=r1&viewer=".to_string(),
            "repo=r1&viewer=has%20space".to_string(),
            "repo=r1&viewer=semi;colon".to_string(),
            format!("repo=r1&viewer={long}"),
        ] {
            let first = browser_viewer(&head(&query));
            let second = browser_viewer(&head(&query));
            assert_ne!(first, second, "each must be its own screen: {query}");
        }
    }

    #[test]
    fn an_id_at_the_cap_is_still_accepted() {
        let id = "a".repeat(MAX_VIEWER_ID);
        assert_eq!(
            browser_viewer(&head(&format!("repo=r1&viewer={id}"))),
            ViewerId::Browser(id)
        );
    }
}
