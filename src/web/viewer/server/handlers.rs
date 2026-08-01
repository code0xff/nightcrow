use super::http_util::json_error;
use super::mutations::{lookup_repo, redact};
use super::{SSE_HEARTBEAT, TERM_POLL_TIMEOUT, ViewerState};
use crate::web::common::conn;
use crate::web::common::sse::SseStream;
use crate::web::viewer::catalog::RepoEntry;
use crate::web::viewer::dto::Envelope;
use crate::web::viewer::size_owner::ViewerId;
use crate::web::viewer::terminal::{self, ClientMessage, TerminalFrame};
use anyhow::{Context, Result};
use std::io::Write;
use std::net::TcpStream;
use std::time::Duration;
use tungstenite::Message;

/// Look the repository up, validate any `path` parameter, then run `body`.
///
/// Validation happens here rather than in each handler so no route can forget
/// it. Not every downstream touches the filesystem — `load_file_diff` passes
/// the path to git as a pathspec — but a route must not be safe only by
/// accident of which loader it happens to call. A traversal path is refused
/// uniformly, and never echoed back in a response.
pub(super) fn with_repo(
    head: &crate::web::common::http::RequestHead,
    state: &ViewerState,
    body: impl FnOnce(&RepoEntry) -> Result<Vec<u8>>,
) -> Vec<u8> {
    let entry = match lookup_repo(head, state) {
        Ok(entry) => entry,
        Err(response) => return response,
    };
    // An absent or empty `path` means "the repository root" for the routes that
    // accept one; anything else has to survive the gate.
    if let Some(path) = head.query_param("path").filter(|p| !p.is_empty())
        && let Err(err) =
            crate::git::path::resolve_in_workdir(std::path::Path::new(&entry.path), &path)
    {
        tracing::debug!(%err, route = %head.path, "viewer: rejected path parameter");
        return json_error("400 Bad Request", "invalid path");
    }
    match body(&entry) {
        Ok(response) => response,
        Err(err) => redact(&head.path, &err),
    }
}

/// Variant of [`with_repo`] for a path inside a historical git object.
///
/// A deleted commit path cannot be resolved in the current worktree, so this
/// validates its syntax without statting it. The route passes it only to an
/// exact git pathspec; it never opens a filesystem path.
pub(super) fn with_repo_commit_path(
    head: &crate::web::common::http::RequestHead,
    state: &ViewerState,
    body: impl FnOnce(&RepoEntry, &str) -> Result<Vec<u8>>,
) -> Vec<u8> {
    let entry = match lookup_repo(head, state) {
        Ok(entry) => entry,
        Err(response) => return response,
    };
    let path = match required_path(head) {
        Ok(path) => path,
        Err(err) => return redact(&head.path, &err),
    };
    if let Err(err) = crate::git::path::validate_commit_path(&path) {
        tracing::debug!(%err, route = %head.path, "viewer: rejected historical path parameter");
        return json_error("400 Bad Request", "invalid path");
    }
    match body(&entry, &path) {
        Ok(response) => response,
        Err(err) => redact(&head.path, &err),
    }
}

pub(super) fn required_path(head: &crate::web::common::http::RequestHead) -> Result<String> {
    head.query_param("path")
        .filter(|p| !p.is_empty())
        .ok_or_else(|| anyhow::anyhow!("missing path parameter"))
}

/// An oid query parameter that may be absent, but must parse when present.
///
/// Absent and malformed are kept apart deliberately: silently walking from HEAD
/// after a typo would answer a different question than the one asked, and the
/// client pages against the value it gets back.
pub(super) fn optional_oid(
    head: &crate::web::common::http::RequestHead,
    name: &str,
) -> Result<Option<git2::Oid>> {
    match head.query_param(name) {
        None => Ok(None),
        Some(text) => git2::Oid::from_str(&text)
            .map(Some)
            .with_context(|| format!("malformed {name} parameter")),
    }
}

/// A non-negative count query parameter, defaulting to zero when absent.
/// Deliberately unbounded — see the note beside [`limits::MAX_LOG_PAGE`].
pub(super) fn optional_count(
    head: &crate::web::common::http::RequestHead,
    name: &str,
) -> Result<usize> {
    let Some(text) = head.query_param(name) else {
        return Ok(0);
    };
    text.parse()
        .with_context(|| format!("malformed {name} parameter"))
}

pub(super) fn required_oid(head: &crate::web::common::http::RequestHead) -> Result<git2::Oid> {
    let oid_text = head
        .query_param("oid")
        .ok_or_else(|| anyhow::anyhow!("missing oid parameter"))?;
    git2::Oid::from_str(&oid_text).context("malformed oid")
}

pub(super) fn open_repo(path: &str) -> Result<git2::Repository> {
    git2::Repository::discover(path).context("failed to open repository")
}

pub(super) fn encode<T: serde::Serialize>(payload: &T) -> Result<String> {
    serde_json::to_string(&Envelope::new(payload)).context("failed to encode payload")
}

/// Hold the connection open and stream this repository's status.
pub(super) fn serve_events(
    mut stream: TcpStream,
    head: &crate::web::common::http::RequestHead,
    state: &ViewerState,
) {
    let entry = match lookup_repo(head, state) {
        Ok(entry) => entry,
        Err(response) => {
            let _ = stream.write_all(&response);
            return;
        }
    };
    // A stalled reader must not wedge the handler thread forever.
    let _ = stream.set_write_timeout(Some(SSE_HEARTBEAT));

    let subscription = entry.runtime.subscribe();
    let Ok(mut sse) = SseStream::start(stream) else {
        return;
    };
    loop {
        match subscription.next_update(SSE_HEARTBEAT) {
            Some(update) => {
                if sse.send("status", &update.json).is_err() {
                    break;
                }
            }
            // Nothing changed: prove the socket is still alive. This is the
            // only way a closed tab is discovered.
            None => {
                if sse.heartbeat().is_err() {
                    break;
                }
            }
        }
    }
    // `subscription` drops here, unregistering from the fan-out.
}

/// The longest `viewer` id accepted. Long enough for a UUID with room to spare;
/// the value is only ever compared, never shown.
const MAX_VIEWER_ID: usize = 64;

/// A page's identity for the session's size ownership, from what it called
/// itself.
///
/// The page generates this once per tab and sends it on every socket, so its
/// connections can come and go — a repository switch, a reconnect — without the
/// session reading them as somebody new sitting down.
///
/// A boundary input, so it is held to what an id can be: a short run of plain
/// characters. An id that is missing or malformed gets one of its own rather
/// than a refusal — the page still works, it simply behaves as it did before it
/// could name itself, and a stale cached bundle is the likely reason.
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
/// repository was named — a terminal is effectively a shell, so the upgrade
/// must never be reachable ahead of those checks.
pub(super) fn serve_terminal(
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
    // repository switch or a reconnect. Absent means no — a socket that does not
    // say it arrived must not take the sizing off whoever is looking.
    let arriving = head.query_param("claim").as_deref() == Some("1");
    // A second handle, kept by the hub only to end this connection if the page
    // stops draining its queue: the loop below is then parked in `ws.read()` and
    // nothing else would wake it. A clone that could not be made costs the hub
    // that ability and nothing else.
    let session = entry
        .terminals
        .connect(browser_viewer(head), arriving, evict_handle.ok());

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

    /// A stale bundle, or something that is not a page at all. It still works —
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
