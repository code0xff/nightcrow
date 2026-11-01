//! One terminal socket's writer: the only thread that parses or writes a frame.

use super::terminal_batch::Batch;
use super::terminal_errors::{note_end, stalled_not_gone};
use super::terminal_liveness::{Liveness, Step, Timing};
use super::terminal_pipe::{Duplex, Event, Link, Waiting};
use crate::session::terminal::{self, ClientMessage, TerminalFrame, TerminalSession};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tungstenite::Message;

/// How long a stalled write is left alone before it is tried again. Only a
/// client that has stopped reading reaches this, and it costs one wake per
/// interval rather than a spin.
const FLUSH_RETRY: Duration = Duration::from_millis(4);

/// Count the passes a connection makes, so a test can show that an idle socket
/// no longer wakes its thread on a timer. Test-only: the loop's real job is
/// visible in what it sends, not in how often it looks.
#[cfg(test)]
pub(super) static WAKES: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

/// Count the frames taken off the hub after a write had already stalled. It has
/// to stay zero: the drain loop's guard is what keeps a client that stopped
/// reading backing up in the hub instead of in this thread. Test-only, because
/// the count is the guard itself rather than anything the wire shows.
#[cfg(test)]
pub(super) static DEQUEUED_WHILE_STALLED: std::sync::atomic::AtomicUsize =
    std::sync::atomic::AtomicUsize::new(0);

/// Why a connection ended.
enum End {
    /// The page sent a close frame.
    Page,
    /// The page's side of the socket ended without one.
    ReaderGone,
    /// The hub let this client go, or the hub itself is gone.
    Hub,
    /// Nothing arrived for the whole silence limit.
    Silent(Duration),
    /// The socket failed during the named operation.
    Failed(tungstenite::Error, &'static str),
}

impl End {
    fn note(&self) {
        match self {
            Self::Page => tracing::info!("viewer: terminal socket closed by the page"),
            Self::ReaderGone => {
                tracing::info!("viewer: terminal socket ended with the page's side")
            }
            Self::Hub => tracing::debug!("viewer: terminal client released by the hub"),
            // WARN: the default log level must be able to say why a panel went
            // blank, and nothing else reports a connection the page never
            // closed.
            Self::Silent(limit) => tracing::warn!(
                silence = ?limit,
                "viewer: terminal socket stopped answering; closing"
            ),
            Self::Failed(err, during) => note_end(err, during),
        }
    }
}

struct Connection {
    ws: tungstenite::WebSocket<Duplex>,
    link: Link,
    session: Arc<TerminalSession>,
    liveness: Liveness,
    batch: Batch,
    /// A flush did not go out in full; tungstenite is holding the rest and the
    /// next pass must move it before anything else is queued.
    unflushed: bool,
}

/// Serve one terminal socket until either side is gone.
///
/// The loop has nothing to poll: it waits on `link` with a timeout that is the
/// distance to the next liveness decision, so an idle connection never wakes
/// this thread. The only standing cost is the reader's guard interval, which is
/// what lets a read in progress end at all.
pub(super) fn run(
    ws: tungstenite::WebSocket<Duplex>,
    link: Link,
    session: Arc<TerminalSession>,
    timing: Timing,
) {
    let mut connection = Connection {
        ws,
        link,
        session,
        liveness: Liveness::new(timing, Instant::now()),
        batch: Batch::default(),
        unflushed: false,
    };
    let end = connection.serve();
    connection.finish(end);
}

impl Connection {
    fn serve(&mut self) -> End {
        loop {
            #[cfg(test)]
            WAKES.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            #[cfg(test)]
            if std::mem::take(&mut self.link.panic_writer) {
                panic!("test: a terminal writer panicked");
            }

            // Everything already waiting leaves in one flush — until a write
            // stalls. Then nothing more leaves the hub: tungstenite's write
            // buffer is unbounded, so dequeuing through it would turn a client
            // that stopped reading into this thread's memory. Backing up in the
            // hub's own bounded queue is what ends such a client.
            while !self.unflushed {
                match self.link.try_next() {
                    Waiting::Event(event) => {
                        #[cfg(test)]
                        if self.unflushed {
                            DEQUEUED_WHILE_STALLED
                                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                        }
                        if let Some(end) = self.apply(event) {
                            return end;
                        }
                        if self.batch.due()
                            && let Some(end) = self.flush()
                        {
                            return end;
                        }
                    }
                    Waiting::Empty => break,
                    Waiting::Closed => return End::Hub,
                }
            }
            if (self.batch.queued() || self.unflushed)
                && let Some(end) = self.flush()
            {
                return end;
            }

            let now = Instant::now();
            match self.liveness.step(now) {
                Step::Wait => {}
                Step::Ping => {
                    if let Some(end) = self.ping(now) {
                        return end;
                    }
                }
                // A client with a write outstanding is not necessarily silent:
                // its pongs and input can be waiting in the channel this thread
                // has stopped draining. Leave it to the hub's overflow, which
                // ends a client that truly stopped keeping up.
                Step::Close if !self.unflushed => {
                    return End::Silent(self.liveness.silence_limit());
                }
                Step::Close => {}
            }

            // Sleep until an event arrives or the next decision is due. A
            // stalled write is retried on a short beat instead, because the
            // socket becoming writable is not an event anyone can deliver.
            let wait = self
                .liveness
                .next_deadline()
                .saturating_duration_since(Instant::now());
            if self.unflushed {
                std::thread::sleep(wait.min(FLUSH_RETRY));
                continue;
            }
            match self.link.next(wait) {
                Waiting::Event(event) => {
                    if let Some(end) = self.apply(event) {
                        return end;
                    }
                }
                Waiting::Empty => {}
                Waiting::Closed => return End::Hub,
            }
        }
    }

    fn apply(&mut self, event: Event) -> Option<End> {
        match event {
            Event::Bytes(bytes) => {
                self.ws.get_mut().feed(&bytes);
                self.read_messages()
            }
            Event::Frame(frame) => {
                let message = match frame {
                    TerminalFrame::Output { pane, data } => {
                        Message::Binary(terminal::encode_output(pane, &data).into())
                    }
                    TerminalFrame::Control(json) => Message::Text(json.into()),
                };
                self.queue(message)
            }
            Event::ClientGone => Some(End::ReaderGone),
        }
    }

    /// Parse every message the fed bytes complete. tungstenite keeps its decode
    /// state across the `WouldBlock` that ends this, so the next chunk resumes
    /// exactly where the last stopped.
    fn read_messages(&mut self) -> Option<End> {
        loop {
            match self.ws.read() {
                Ok(message) => {
                    // Any frame — a pong, pane input, a close — proves the path
                    // still carries frames, even if it says nothing.
                    self.liveness.heard(Instant::now());
                    match message {
                        Message::Text(text) => match serde_json::from_str::<ClientMessage>(&text) {
                            Ok(message) => self.session.dispatch(message),
                            // A malformed frame is dropped, not fatal: a client
                            // bug should not take the terminal down with it.
                            Err(err) => tracing::debug!(%err, "viewer: bad terminal message"),
                        },
                        Message::Close(_) => return Some(End::Page),
                        _ => {}
                    }
                }
                Err(err) if stalled_not_gone(&err) => return None,
                Err(err) => return Some(End::Failed(err, "read")),
            }
        }
    }

    fn queue(&mut self, message: Message) -> Option<End> {
        let len = message.len();
        match self.ws.write(message) {
            Ok(()) => {
                self.batch.add(len);
                None
            }
            // tungstenite buffered the frame and will try again on the next
            // flush; let the retry beat move it.
            Err(err) if stalled_not_gone(&err) => {
                self.batch.add(len);
                self.unflushed = true;
                None
            }
            Err(err) => Some(End::Failed(err, "write")),
        }
    }

    fn flush(&mut self) -> Option<End> {
        match self.ws.flush() {
            Ok(()) => {
                self.unflushed = false;
                self.batch.clear();
                None
            }
            Err(err) if stalled_not_gone(&err) => {
                self.unflushed = true;
                self.batch.clear();
                None
            }
            Err(err) => Some(End::Failed(err, "flush")),
        }
    }

    fn ping(&mut self, now: Instant) -> Option<End> {
        match self.ws.write(Message::Ping(Vec::new().into())) {
            Ok(()) => {
                let end = self.flush();
                self.liveness.pinged(now);
                end
            }
            Err(err) if stalled_not_gone(&err) => {
                self.liveness.pinged(now);
                self.unflushed = true;
                None
            }
            Err(err) => Some(End::Failed(err, "ping")),
        }
    }

    /// Say goodbye. The helper threads and the hub client are released by
    /// [`Link`]'s `Drop`, which runs on every exit path, including a panic.
    ///
    /// The socket is shut down before the close frame. tungstenite's `close`
    /// flushes, and a flush on the stalled socket a silent end exists for would
    /// block for the whole write timeout before the shutdown it is meant to
    /// precede. Nothing is waiting on a close frame the page has stopped
    /// reading.
    fn finish(mut self, end: End) {
        end.note();
        self.ws.get_ref().cut_off();
        if let End::Silent(_) = end {
            let _ = self.ws.close(None);
        }
    }
}
