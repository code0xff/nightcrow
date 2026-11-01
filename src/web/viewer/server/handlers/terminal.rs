use super::super::mutations::lookup_repo;
use super::super::{SSE_HEARTBEAT, TERM_POLL_TIMEOUT, ViewerState};
use super::terminal_errors::{note_end, stalled_not_gone};
use super::terminal_liveness::{Liveness, Step, Timing};
use crate::session::size_owner::ViewerId;
use crate::session::terminal::{self, ClientMessage, TerminalFrame, TerminalSession};
use crate::web::common::conn;
use std::io::Write;
use std::net::TcpStream;
use std::time::{Duration, Instant};
use tungstenite::Message;

/// The longest `viewer` id accepted. Long enough for a UUID with room to spare.
const MAX_VIEWER_ID: usize = 64;

/// Bounds on how much output one drain may queue before flushing. A pane that
/// writes more often than the drain poll would otherwise keep the drain going
/// and hold everything it produced until the burst ends.
const FLUSH_FRAMES: usize = 64;
const FLUSH_BYTES: usize = 32 * 1024;
const FLUSH_AFTER: Duration = Duration::from_millis(4);

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

/// Count the passes a connection makes, so a test can show that an idle socket
/// no longer wakes its thread on a timer. Test-only: the loop's real job is
/// visible in what it sends, not in how often it looks.
#[cfg(test)]
pub(super) static WAKES: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

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
    // Terminal frames are small and latency matters more than packing them:
    // Nagle would hold a keystroke's echo until an unrelated write completed.
    let _ = stream.set_nodelay(true);
    // Taken before the stream is moved into the websocket, which owns it from
    // there on.
    let evict_handle = stream.try_clone();
    let Some(ws) = conn::websocket_handshake(stream, head) else {
        return;
    };
    // `claim` is the page saying a person just opened it, as opposed to a
    // repository switch or a reconnect. Absent means no — a socket that does
    // not say it arrived must not take the sizing off whoever is looking.
    let arriving = head.query_param("claim").as_deref() == Some("1");
    // A second handle, kept by the hub only to end this connection if the page
    // stops draining its queue: the loop below is then parked in `ws.read()`
    // and nothing else would wake it. A clone that could not be made costs the
    // hub that ability and nothing else.
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
    let session = entry
        .terminals
        .connect(browser_viewer(head), arriving, evict_handle);
    // Paired with `note_end`: between the two the log holds this connection's
    // whole span, and the repo names which panel it was — a page cycling
    // sockets shows here as rapid attach lines for one viewer.
    tracing::info!(
        repo = head.query_param("repo").as_deref().unwrap_or("?"),
        viewer = ?browser_viewer(head),
        arriving,
        "viewer: terminal socket attached"
    );

    run(ws, session, Timing::default());
}

/// The connection's whole life: pump this client's frames out, listen for its
/// input, and end the socket once neither side is there.
///
/// `timing` is the caller's so a test can watch a timeout elapse quickly; the
/// only production value is [`Timing::default`].
fn run(mut ws: tungstenite::WebSocket<TcpStream>, session: TerminalSession, timing: Timing) {
    let mut liveness = Liveness::new(timing, Instant::now());
    // Set when a write could not go out in full. tungstenite is then holding
    // the rest, and only a later flush moves it -- so this has to be retried
    // even if no new output ever arrives.
    let mut unflushed = false;
    loop {
        #[cfg(test)]
        WAKES.fetch_add(1, std::sync::atomic::Ordering::Relaxed);

        let mut wrote = false;
        let mut wrote_anything = false;
        let mut batch_frames = 0;
        let mut batch_bytes = 0;
        let mut batch_started: Option<Instant> = None;
        // Drain everything queued for us before blocking on the socket, so
        // output is not held back waiting for the client to say something.
        while let Some(frame) = session.next_frame(Duration::from_millis(1)) {
            let message = match frame {
                TerminalFrame::Output { pane, data } => {
                    Message::Binary(terminal::encode_output(pane, &data).into())
                }
                TerminalFrame::Control(json) => Message::Text(json.into()),
            };
            // `write` queues without flushing, so a burst leaves as one flush
            // below instead of a syscall pair per frame.
            let len = message.len();
            match ws.write(message) {
                Ok(()) => {
                    wrote = true;
                    wrote_anything = true;
                    batch_frames += 1;
                    batch_bytes += len;
                    let started = *batch_started.get_or_insert_with(Instant::now);
                    if batch_frames >= FLUSH_FRAMES
                        || batch_bytes >= FLUSH_BYTES
                        || started.elapsed() >= FLUSH_AFTER
                    {
                        match ws.flush() {
                            Ok(()) => {
                                wrote = false;
                                batch_frames = 0;
                                batch_bytes = 0;
                                batch_started = None;
                            }
                            Err(err) if stalled_not_gone(&err) => {
                                unflushed = true;
                                break;
                            }
                            Err(err) => return note_end(&err, "flush"),
                        }
                    }
                }
                // Stop pulling from the hub while the socket will not take it,
                // so what is still queued backs up where the cap is: the hub's
                // own queue, whose overflow disconnects a client that has
                // really stopped keeping up.
                Err(err) if stalled_not_gone(&err) => {
                    unflushed = true;
                    break;
                }
                Err(err) => return note_end(&err, "write"),
            }
        }
        if wrote || unflushed {
            match ws.flush() {
                Ok(()) => unflushed = false,
                Err(err) if stalled_not_gone(&err) => unflushed = true,
                Err(err) => return note_end(&err, "flush"),
            }
        }
        if wrote_anything {
            liveness.sent(Instant::now());
        }

        match liveness.step(Instant::now()) {
            Step::Wait => {}
            Step::Ping => {
                // Only `WouldBlock`/`TimedOut` are recoverable; anything else
                // means the socket is gone and there is nobody to ask.
                match ws.write(Message::Ping(Vec::new().into())) {
                    Ok(()) => match ws.flush() {
                        Ok(()) => liveness.sent(Instant::now()),
                        Err(err) if stalled_not_gone(&err) => {
                            // The ping is in the write buffer; the next flush
                            // sends it. Recording it now keeps the loop from
                            // queueing another on every pass.
                            liveness.sent(Instant::now());
                            unflushed = true;
                        }
                        Err(err) => return note_end(&err, "ping"),
                    },
                    Err(err) if stalled_not_gone(&err) => liveness.sent(Instant::now()),
                    Err(err) => return note_end(&err, "ping"),
                }
            }
            Step::Close => {
                // The page has sent nothing at all for two intervals, and a
                // ping did not bring an answer. Closing releases the hub
                // client, this connection's queue, and its slot in the
                // viewer's connection cap; the page reconnects and replays.
                tracing::warn!(
                    silence = ?timing.silence_limit,
                    "viewer: terminal socket stopped answering; closing"
                );
                let _ = ws.close(None);
                return;
            }
        }

        match ws.read() {
            Ok(message) => {
                liveness.heard(Instant::now());
                match message {
                    Message::Text(text) => match serde_json::from_str::<ClientMessage>(&text) {
                        Ok(message) => session.dispatch(message),
                        // A malformed frame is dropped, not fatal: a client bug
                        // should not take the terminal down with it.
                        Err(err) => tracing::debug!(%err, "viewer: bad terminal message"),
                    },
                    Message::Close(_) => {
                        tracing::info!("viewer: terminal socket closed by the page");
                        return;
                    }
                    // A pong is the answer a ping asked for; a ping is the page
                    // asking the same of us. Either is proof the path carries
                    // frames, which `heard` has already recorded.
                    _ => {}
                }
            }
            // A poll timeout surfaces as WouldBlock on macOS and TimedOut on
            // Linux; neither means the client is gone.
            Err(err) if stalled_not_gone(&err) => {}
            Err(err) => return note_end(&err, "read"),
        }
    }
    // `session` drops here, unregistering from the hub.
}

#[cfg(test)]
#[path = "terminal_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "terminal_socket_tests.rs"]
mod socket_tests;
