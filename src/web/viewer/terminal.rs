//! Terminals owned by the viewer, one hub per repository.
//!
//! These are **not** the TUI's panes. Sharing them would mean reaching into
//! `App`, which would break both the "server never touches App" rule and the
//! headless mode. The viewer owns its own [`PtyBackend`], so `nightcrow serve`
//! offers terminals with no TUI running at all.
//!
//! Raw PTY bytes go to the browser untouched — no server-side VT emulation.
//! xterm.js is a terminal emulator already; the mirror only composes a grid
//! because it has to match a ratatui screen, and this has no such constraint.
//!
//! **Output is queued, not conflated.** Status updates can drop intermediates
//! because the newest is a complete picture; terminal bytes cannot — dropping
//! any corrupts the stream. Each client gets a bounded queue and is
//! disconnected when it overflows, which is honest, where silently discarding
//! bytes would leave a subtly wrong screen.

use crate::backend::{BackendEvent, PaneId, PtyBackend, TerminalBackend};
use crate::web::viewer::limits;
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, SyncSender, TrySendError};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

/// How often the hub thread services commands and drains PTY output. Terminal
/// latency is felt directly, so this is much tighter than the status poll.
const POLL_INTERVAL: Duration = Duration::from_millis(8);

/// Output frames a client may fall behind by before it is dropped.
const CLIENT_QUEUE_DEPTH: usize = 256;

/// A control message from the browser. Output travels as binary frames
/// instead, so it never pays JSON escaping or base64 expansion.
#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum ClientMessage {
    Create { rows: u16, cols: u16 },
    Input { pane: PaneId, data: String },
    Resize { pane: PaneId, rows: u16, cols: u16 },
    Close { pane: PaneId },
}

/// A control message to the browser.
#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum ServerMessage {
    Created { pane: PaneId },
    Exited { pane: PaneId },
    Error { message: String },
}

/// One frame queued for a connected client.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TerminalFrame {
    /// Raw PTY bytes for `pane`. Sent as a binary WebSocket frame with the
    /// pane id prefixed, so one socket multiplexes every terminal losslessly.
    Output { pane: PaneId, data: Vec<u8> },
    /// A JSON control frame.
    Control(String),
}

/// Encode an output frame: 4-byte little-endian pane id, then the raw bytes.
///
/// Binary rather than JSON because PTY output is not guaranteed valid UTF-8 —
/// a multi-byte sequence is routinely split across reads, and lossy decoding
/// would corrupt it before xterm.js ever reassembles it.
pub fn encode_output(pane: PaneId, data: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(data.len() + 4);
    out.extend_from_slice(&pane.to_le_bytes());
    out.extend_from_slice(data);
    out
}

/// Decode an output frame produced by [`encode_output`].
pub fn decode_output(frame: &[u8]) -> Option<(PaneId, &[u8])> {
    if frame.len() < 4 {
        return None;
    }
    let (id_bytes, rest) = frame.split_at(4);
    let pane = PaneId::from_le_bytes(id_bytes.try_into().ok()?);
    Some((pane, rest))
}

enum Command {
    Create { rows: u16, cols: u16, client: u64 },
    Input { pane: PaneId, data: Vec<u8> },
    Resize { pane: PaneId, rows: u16, cols: u16 },
    Close { pane: PaneId },
}

struct Client {
    id: u64,
    tx: SyncSender<TerminalFrame>,
}

/// A client's connection to a repository's terminals.
pub struct TerminalSession {
    hub: Arc<TerminalHub>,
    id: u64,
    rx: Receiver<TerminalFrame>,
}

impl TerminalSession {
    /// Wait up to `timeout` for the next frame to write.
    pub fn next_frame(&self, timeout: Duration) -> Option<TerminalFrame> {
        self.rx.recv_timeout(timeout).ok()
    }

    /// Handle a decoded control message from this client.
    pub fn dispatch(&self, message: ClientMessage) {
        let command = match message {
            ClientMessage::Create { rows, cols } => Command::Create {
                rows: rows.max(1),
                cols: cols.max(1),
                client: self.id,
            },
            ClientMessage::Input { pane, data } => Command::Input {
                pane,
                data: data.into_bytes(),
            },
            ClientMessage::Resize { pane, rows, cols } => Command::Resize {
                pane,
                rows: rows.max(1),
                cols: cols.max(1),
            },
            ClientMessage::Close { pane } => Command::Close { pane },
        };
        // Never block the connection thread here. The hub drains this queue
        // from the same thread that writes to a PTY master, and that write
        // blocks forever if the child has stopped reading stdin — a blocking
        // send would then wedge every connection thread for this repository.
        // Dropping a command under that much backpressure is the honest
        // outcome; the client is already far ahead of what the shell can take.
        if let Err(TrySendError::Full(_)) = self.hub.commands.try_send(command) {
            tracing::debug!("viewer: terminal command queue full, dropping");
        }
    }
}

impl Drop for TerminalSession {
    fn drop(&mut self) {
        self.hub.disconnect(self.id);
    }
}

pub struct TerminalHub {
    commands: SyncSender<Command>,
    clients: Mutex<Vec<Client>>,
    next_client_id: AtomicU64,
    stop: Arc<AtomicBool>,
    worker: Mutex<Option<thread::JoinHandle<()>>>,
}

impl TerminalHub {
    /// Start a hub whose terminals run in `cwd`.
    pub fn spawn(cwd: &str) -> Arc<Self> {
        let (commands, command_rx) = mpsc::sync_channel::<Command>(256);
        let hub = Arc::new(Self {
            commands,
            clients: Mutex::new(Vec::new()),
            next_client_id: AtomicU64::new(0),
            stop: Arc::new(AtomicBool::new(false)),
            worker: Mutex::new(None),
        });

        let worker_hub = Arc::clone(&hub);
        let stop = Arc::clone(&hub.stop);
        let cwd = cwd.to_string();
        let handle = thread::Builder::new()
            .name("nightcrow-viewer-term".into())
            .spawn(move || worker_hub.run(&cwd, command_rx, stop))
            .ok();
        *hub.worker.lock().expect("terminal worker slot poisoned") = handle;
        hub
    }

    fn run(&self, cwd: &str, commands: Receiver<Command>, stop: Arc<AtomicBool>) {
        let mut backend = PtyBackend::new(cwd);
        let mut live: Vec<PaneId> = Vec::new();

        while !stop.load(Ordering::Acquire) {
            while let Ok(command) = commands.try_recv() {
                match command {
                    Command::Create { rows, cols, client } => {
                        if live.len() >= limits::MAX_PTYS_PER_REPO {
                            self.send_control(
                                Some(client),
                                &ServerMessage::Error {
                                    message: "terminal limit reached".to_string(),
                                },
                            );
                            continue;
                        }
                        match backend.create_pane(rows, cols, None) {
                            Ok(pane) => {
                                live.push(pane);
                                // Broadcast: every client shows the same set of
                                // terminals for this repository.
                                self.send_control(None, &ServerMessage::Created { pane });
                            }
                            Err(err) => {
                                tracing::warn!(%err, "viewer: could not create a terminal");
                                self.send_control(
                                    Some(client),
                                    &ServerMessage::Error {
                                        message: "could not start a terminal".to_string(),
                                    },
                                );
                            }
                        }
                    }
                    // Unknown pane ids are ignored rather than errored: a
                    // client racing a pane exit is normal, not an attack.
                    Command::Input { pane, data } if live.contains(&pane) => {
                        let _ = backend.send_input(pane, &data);
                    }
                    Command::Resize { pane, rows, cols } if live.contains(&pane) => {
                        backend.resize(pane, rows, cols);
                    }
                    Command::Close { pane } if live.contains(&pane) => {
                        backend.destroy_pane(pane);
                        live.retain(|p| *p != pane);
                        self.send_control(None, &ServerMessage::Exited { pane });
                    }
                    _ => {}
                }
            }

            for event in backend.drain_events() {
                match event {
                    BackendEvent::Output { pane, data } => {
                        self.broadcast(TerminalFrame::Output { pane, data });
                    }
                    BackendEvent::Exited { pane } => {
                        live.retain(|p| *p != pane);
                        self.send_control(None, &ServerMessage::Exited { pane });
                    }
                }
            }
            thread::sleep(POLL_INTERVAL);
        }

        for pane in live {
            backend.destroy_pane(pane);
        }
    }

    fn send_control(&self, only: Option<u64>, message: &ServerMessage) {
        let Ok(json) = serde_json::to_string(message) else {
            return;
        };
        match only {
            Some(id) => self.send_to(id, TerminalFrame::Control(json)),
            None => self.broadcast(TerminalFrame::Control(json)),
        }
    }

    fn send_to(&self, id: u64, frame: TerminalFrame) {
        let mut clients = self.clients.lock().expect("terminal clients poisoned");
        if let Some(index) = clients.iter().position(|c| c.id == id)
            && clients[index].tx.try_send(frame).is_err()
        {
            clients.remove(index);
        }
    }

    /// Queue a frame for every client, dropping any that has fallen too far
    /// behind. Terminal bytes cannot be skipped, so an overfull client is
    /// disconnected rather than served a corrupted stream.
    fn broadcast(&self, frame: TerminalFrame) {
        let mut clients = self.clients.lock().expect("terminal clients poisoned");
        clients.retain(|client| match client.tx.try_send(frame.clone()) {
            Ok(()) => true,
            Err(TrySendError::Full(_)) => {
                tracing::debug!(id = client.id, "viewer: terminal client too slow, dropping");
                false
            }
            Err(TrySendError::Disconnected(_)) => false,
        });
    }

    pub fn connect(self: &Arc<Self>) -> TerminalSession {
        let id = self.next_client_id.fetch_add(1, Ordering::AcqRel);
        let (tx, rx) = mpsc::sync_channel(CLIENT_QUEUE_DEPTH);
        self.clients
            .lock()
            .expect("terminal clients poisoned")
            .push(Client { id, tx });
        TerminalSession {
            hub: Arc::clone(self),
            id,
            rx,
        }
    }

    fn disconnect(&self, id: u64) {
        self.clients
            .lock()
            .expect("terminal clients poisoned")
            .retain(|c| c.id != id);
    }

    pub fn client_count(&self) -> usize {
        self.clients
            .lock()
            .expect("terminal clients poisoned")
            .len()
    }

    pub fn stop(&self) {
        self.stop.store(true, Ordering::Release);
        let handle = self
            .worker
            .lock()
            .expect("terminal worker slot poisoned")
            .take();
        if let Some(handle) = handle {
            crate::util::try_timed_join(handle, crate::util::REAP_TIMEOUT);
        }
    }
}

impl Drop for TerminalHub {
    fn drop(&mut self) {
        self.stop();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wait_for<T>(mut take: impl FnMut() -> Option<T>) -> Option<T> {
        for _ in 0..200 {
            if let Some(value) = take() {
                return Some(value);
            }
            thread::sleep(Duration::from_millis(10));
        }
        None
    }

    /// Pull frames until one satisfies `want`, ignoring the rest.
    fn next_matching(
        session: &TerminalSession,
        mut want: impl FnMut(&TerminalFrame) -> bool,
    ) -> Option<TerminalFrame> {
        wait_for(|| {
            session
                .next_frame(Duration::from_millis(50))
                .filter(|f| want(f))
        })
    }

    #[test]
    fn output_frames_round_trip_through_the_binary_encoding() {
        // Raw PTY bytes are not always valid UTF-8; the framing must not care.
        let payload = vec![0x1b, b'[', b'0', b'm', 0xff, 0xfe, 0x00];

        let encoded = encode_output(7, &payload);
        let (pane, data) = decode_output(&encoded).unwrap();

        assert_eq!(pane, 7);
        assert_eq!(data, &payload[..]);
    }

    #[test]
    fn decode_output_rejects_a_frame_too_short_to_carry_a_pane_id() {
        assert!(decode_output(&[]).is_none());
        assert!(decode_output(&[1, 2, 3]).is_none());
        assert_eq!(decode_output(&[1, 0, 0, 0]), Some((1, &[][..])));
    }

    #[test]
    fn client_messages_parse_from_the_wire_shape() {
        let create: ClientMessage =
            serde_json::from_str(r#"{"type":"create","rows":24,"cols":80}"#).unwrap();
        assert!(matches!(
            create,
            ClientMessage::Create { rows: 24, cols: 80 }
        ));

        let input: ClientMessage =
            serde_json::from_str(r#"{"type":"input","pane":3,"data":"ls\n"}"#).unwrap();
        assert!(matches!(input, ClientMessage::Input { pane: 3, .. }));

        assert!(serde_json::from_str::<ClientMessage>(r#"{"type":"nope"}"#).is_err());
        assert!(serde_json::from_str::<ClientMessage>(r#"{"type":"create"}"#).is_err());
    }

    #[test]
    fn server_messages_serialize_with_a_type_tag() {
        let json = serde_json::to_string(&ServerMessage::Created { pane: 2 }).unwrap();
        assert_eq!(json, r#"{"type":"created","pane":2}"#);
    }

    #[test]
    fn creating_a_terminal_announces_it_and_streams_output() {
        let dir = tempfile::TempDir::new().unwrap();
        let hub = TerminalHub::spawn(&dir.path().to_string_lossy());
        let session = hub.connect();

        session.dispatch(ClientMessage::Create { rows: 24, cols: 80 });

        let created = next_matching(
            &session,
            |f| matches!(f, TerminalFrame::Control(json) if json.contains("created")),
        );
        assert!(created.is_some(), "no created message");

        // A real shell writes a prompt; that is enough to prove the PTY is live.
        let output = next_matching(&session, |f| matches!(f, TerminalFrame::Output { .. }));
        assert!(output.is_some(), "no output from the shell");
        hub.stop();
    }

    #[test]
    fn the_per_repo_terminal_cap_is_enforced() {
        let dir = tempfile::TempDir::new().unwrap();
        let hub = TerminalHub::spawn(&dir.path().to_string_lossy());
        let session = hub.connect();

        for _ in 0..limits::MAX_PTYS_PER_REPO + 2 {
            session.dispatch(ClientMessage::Create { rows: 24, cols: 80 });
        }

        let refused = next_matching(
            &session,
            |f| matches!(f, TerminalFrame::Control(json) if json.contains("terminal limit reached")),
        );
        assert!(
            refused.is_some(),
            "the cap must refuse the extra terminals, not spawn them"
        );
        hub.stop();
    }

    #[test]
    fn a_dropped_session_stops_receiving() {
        let dir = tempfile::TempDir::new().unwrap();
        let hub = TerminalHub::spawn(&dir.path().to_string_lossy());

        let session = hub.connect();
        assert_eq!(hub.client_count(), 1);
        drop(session);

        assert_eq!(hub.client_count(), 0);
        hub.stop();
    }

    #[test]
    fn input_for_an_unknown_pane_is_ignored() {
        // A client racing a pane exit is normal traffic, not an error worth
        // tearing the connection down for.
        let dir = tempfile::TempDir::new().unwrap();
        let hub = TerminalHub::spawn(&dir.path().to_string_lossy());
        let session = hub.connect();

        session.dispatch(ClientMessage::Input {
            pane: 9999,
            data: "rm -rf /\n".to_string(),
        });
        session.dispatch(ClientMessage::Resize {
            pane: 9999,
            rows: 10,
            cols: 10,
        });
        session.dispatch(ClientMessage::Close { pane: 9999 });

        // The hub must still be serving after all three.
        session.dispatch(ClientMessage::Create { rows: 24, cols: 80 });
        let created = next_matching(
            &session,
            |f| matches!(f, TerminalFrame::Control(json) if json.contains("created")),
        );
        assert!(created.is_some(), "the hub stopped serving");
        hub.stop();
    }

    #[test]
    fn stop_is_idempotent() {
        let dir = tempfile::TempDir::new().unwrap();
        let hub = TerminalHub::spawn(&dir.path().to_string_lossy());
        hub.stop();
        hub.stop();
    }
}
