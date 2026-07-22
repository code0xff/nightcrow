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
use std::collections::VecDeque;
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
    /// `command` is `Some` only for startup panes (run via `$SHELL -lc`);
    /// client-initiated creates always pass `None` for a bare interactive shell.
    Create {
        rows: u16,
        cols: u16,
        client: u64,
        command: Option<String>,
    },
    Input { pane: PaneId, data: Vec<u8> },
    Resize { pane: PaneId, rows: u16, cols: u16 },
    Close { pane: PaneId },
}

struct Client {
    id: u64,
    tx: SyncSender<TerminalFrame>,
}

/// A live terminal and the recent raw bytes it has produced, kept so a client
/// that connects (or reconnects after a refresh) can be replayed the current
/// screen. Bounded by [`limits::MAX_TERMINAL_SCROLLBACK_BYTES`].
struct PaneState {
    id: PaneId,
    scrollback: VecDeque<u8>,
}

/// Hub state shared between the worker thread (which mutates panes and
/// broadcasts) and connection threads (which register/unregister clients and
/// snapshot scrollback on connect). Held under one mutex so a connecting
/// client's replay is atomic with the worker's append-and-broadcast: it sees
/// each pane's scrollback exactly once, with no gap before or duplicate of the
/// live output that follows.
struct Shared {
    clients: Vec<Client>,
    panes: Vec<PaneState>,
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
                command: None,
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
    state: Mutex<Shared>,
    next_client_id: AtomicU64,
    stop: Arc<AtomicBool>,
    worker: Mutex<Option<thread::JoinHandle<()>>>,
    /// Commands to run in startup terminals, spawned once when the first client
    /// connects. Empty means a single bare shell (matching the TUI's default).
    startup: Vec<String>,
    /// Set the first time a client connects, so the startup terminals spawn
    /// exactly once for the hub's life rather than on every (re)connection.
    started: AtomicBool,
}

impl TerminalHub {
    /// Start a hub whose terminals run in `cwd`. `startup` is the list of
    /// commands to launch when the first client connects (empty = one shell).
    pub fn spawn(cwd: &str, startup: Vec<String>) -> Arc<Self> {
        let (commands, command_rx) = mpsc::sync_channel::<Command>(256);
        let hub = Arc::new(Self {
            commands,
            state: Mutex::new(Shared {
                clients: Vec::new(),
                panes: Vec::new(),
            }),
            next_client_id: AtomicU64::new(0),
            stop: Arc::new(AtomicBool::new(false)),
            worker: Mutex::new(None),
            startup,
            started: AtomicBool::new(false),
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

        while !stop.load(Ordering::Acquire) {
            while let Ok(command) = commands.try_recv() {
                match command {
                    Command::Create {
                        rows,
                        cols,
                        client,
                        command,
                    } => {
                        if self.pane_count() >= limits::MAX_PTYS_PER_REPO {
                            self.send_error_to(client, "terminal limit reached");
                            continue;
                        }
                        match backend.create_pane(rows, cols, command.as_deref()) {
                            Ok(pane) => self.register_pane(pane),
                            Err(err) => {
                                tracing::warn!(%err, "viewer: could not create a terminal");
                                self.send_error_to(client, "could not start a terminal");
                            }
                        }
                    }
                    // Unknown pane ids are ignored rather than errored: a
                    // client racing a pane exit is normal, not an attack.
                    Command::Input { pane, data } if self.pane_is_live(pane) => {
                        let _ = backend.send_input(pane, &data);
                    }
                    Command::Resize { pane, rows, cols } if self.pane_is_live(pane) => {
                        backend.resize(pane, rows, cols);
                    }
                    Command::Close { pane } if self.pane_is_live(pane) => {
                        backend.destroy_pane(pane);
                        self.remove_pane_and_announce(pane);
                    }
                    _ => {}
                }
            }

            for event in backend.drain_events() {
                match event {
                    BackendEvent::Output { pane, data } => self.record_and_broadcast(pane, data),
                    BackendEvent::Exited { pane } => self.remove_pane_and_announce(pane),
                }
            }
            thread::sleep(POLL_INTERVAL);
        }

        let ids: Vec<PaneId> = self
            .state
            .lock()
            .expect("terminal state poisoned")
            .panes
            .iter()
            .map(|p| p.id)
            .collect();
        for pane in ids {
            backend.destroy_pane(pane);
        }
        // Drop the pane records too: the hub struct can outlive its worker
        // behind an `Arc`, and a late `connect` must not replay these now-dead
        // terminals.
        self.state
            .lock()
            .expect("terminal state poisoned")
            .panes
            .clear();
    }

    fn pane_count(&self) -> usize {
        self.state
            .lock()
            .expect("terminal state poisoned")
            .panes
            .len()
    }

    fn pane_is_live(&self, pane: PaneId) -> bool {
        self.state
            .lock()
            .expect("terminal state poisoned")
            .panes
            .iter()
            .any(|p| p.id == pane)
    }

    /// Record a new pane and announce it to every client. Broadcasting under the
    /// same lock that adds the pane keeps it consistent with `connect`'s replay:
    /// a client either sees this pane via `connect` or via this broadcast, never
    /// both and never neither.
    fn register_pane(&self, pane: PaneId) {
        let json = serde_json::to_string(&ServerMessage::Created { pane }).ok();
        let mut state = self.state.lock().expect("terminal state poisoned");
        state.panes.push(PaneState {
            id: pane,
            scrollback: VecDeque::new(),
        });
        if let Some(json) = json {
            broadcast_locked(&mut state.clients, TerminalFrame::Control(json));
        }
    }

    /// Append output to the pane's bounded scrollback and broadcast it, both
    /// under one lock so a concurrently connecting client cannot slip a replay
    /// snapshot between the append and the broadcast.
    fn record_and_broadcast(&self, pane: PaneId, data: Vec<u8>) {
        let mut state = self.state.lock().expect("terminal state poisoned");
        if let Some(p) = state.panes.iter_mut().find(|p| p.id == pane) {
            push_scrollback(&mut p.scrollback, &data);
        }
        broadcast_locked(&mut state.clients, TerminalFrame::Output { pane, data });
    }

    /// Drop a pane and tell every client, but only if it was still live — a pane
    /// closed by command and then reported `Exited` by the backend must announce
    /// once, not twice.
    fn remove_pane_and_announce(&self, pane: PaneId) {
        let json = serde_json::to_string(&ServerMessage::Exited { pane }).ok();
        let mut state = self.state.lock().expect("terminal state poisoned");
        let existed = state.panes.iter().any(|p| p.id == pane);
        if !existed {
            return;
        }
        state.panes.retain(|p| p.id != pane);
        if let Some(json) = json {
            broadcast_locked(&mut state.clients, TerminalFrame::Control(json));
        }
    }

    fn send_error_to(&self, client_id: u64, message: &str) {
        let Ok(json) = serde_json::to_string(&ServerMessage::Error {
            message: message.to_string(),
        }) else {
            return;
        };
        let mut state = self.state.lock().expect("terminal state poisoned");
        if let Some(index) = state.clients.iter().position(|c| c.id == client_id)
            && state.clients[index]
                .tx
                .try_send(TerminalFrame::Control(json))
                .is_err()
        {
            state.clients.remove(index);
        }
    }

    /// Register a client and replay the current terminals to it before it is
    /// eligible for broadcasts: one `Created` plus one scrollback `Output` per
    /// live pane. Done under the state lock so this snapshot cannot interleave
    /// with the worker's append-and-broadcast (see [`Shared`]); the client
    /// therefore receives every pane's history exactly once and in order ahead
    /// of the live stream. A fresh hub (e.g. after a server restart) has no
    /// panes, so a reconnecting client correctly comes back to an empty panel.
    pub fn connect(self: &Arc<Self>) -> TerminalSession {
        let id = self.next_client_id.fetch_add(1, Ordering::AcqRel);
        let (tx, rx) = mpsc::sync_channel(CLIENT_QUEUE_DEPTH);
        let mut state = self.state.lock().expect("terminal state poisoned");
        // A hub whose worker has stopped (its repo was retired) still lingers
        // behind the `Arc` a racing connection resolved, but its panes are dead
        // and will never emit another frame. Skip the replay so the client is
        // not handed phantom terminals it can never receive output or an exit
        // for.
        if !self.stop.load(Ordering::Acquire) {
            for pane in &state.panes {
                if let Ok(json) = serde_json::to_string(&ServerMessage::Created { pane: pane.id }) {
                    let _ = tx.try_send(TerminalFrame::Control(json));
                }
                if !pane.scrollback.is_empty() {
                    let data: Vec<u8> = pane.scrollback.iter().copied().collect();
                    let _ = tx.try_send(TerminalFrame::Output { pane: pane.id, data });
                }
            }
        }
        state.clients.push(Client { id, tx });
        drop(state);

        // First connection spawns the startup terminals (once per hub life):
        // the configured commands, or a single bare shell if none. Queued after
        // the client is registered so it receives the resulting "created"
        // broadcasts, and skipped on a stopped hub.
        if !self.stop.load(Ordering::Acquire)
            && !self.started.swap(true, Ordering::AcqRel)
        {
            if self.startup.is_empty() {
                self.queue_startup_pane(id, None);
            } else {
                for command in &self.startup {
                    self.queue_startup_pane(id, Some(command.clone()));
                }
            }
        }

        TerminalSession {
            hub: Arc::clone(self),
            id,
            rx,
        }
    }

    /// Enqueue a startup terminal. Uses the same command queue as client
    /// creates; a full queue just drops it (the hub is under heavy backpressure,
    /// and a startup pane is not worth wedging the connection thread over).
    fn queue_startup_pane(&self, client: u64, command: Option<String>) {
        let _ = self.commands.try_send(Command::Create {
            rows: 24,
            cols: 80,
            client,
            command,
        });
    }

    fn disconnect(&self, id: u64) {
        self.state
            .lock()
            .expect("terminal state poisoned")
            .clients
            .retain(|c| c.id != id);
    }

    pub fn client_count(&self) -> usize {
        self.state
            .lock()
            .expect("terminal state poisoned")
            .clients
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

/// Queue a frame for every client, dropping any that has fallen too far behind.
/// Terminal bytes cannot be skipped, so an overfull client is disconnected
/// rather than served a corrupted stream. Operates on an already-locked client
/// list so the caller can pair it with a pane mutation atomically.
fn broadcast_locked(clients: &mut Vec<Client>, frame: TerminalFrame) {
    clients.retain(|client| match client.tx.try_send(frame.clone()) {
        Ok(()) => true,
        Err(TrySendError::Full(_)) => {
            tracing::debug!(id = client.id, "viewer: terminal client too slow, dropping");
            false
        }
        Err(TrySendError::Disconnected(_)) => false,
    });
}

/// Append raw PTY bytes to a pane's scrollback, evicting the oldest bytes to
/// stay within [`limits::MAX_TERMINAL_SCROLLBACK_BYTES`].
fn push_scrollback(buf: &mut VecDeque<u8>, data: &[u8]) {
    buf.extend(data.iter().copied());
    if buf.len() > limits::MAX_TERMINAL_SCROLLBACK_BYTES {
        let excess = buf.len() - limits::MAX_TERMINAL_SCROLLBACK_BYTES;
        buf.drain(0..excess);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;

    /// Deadline for the real-shell tests below. `connect` spawns the user's
    /// actual `$SHELL` (an interactive zsh sources its full rc chain), and
    /// cargo runs tests in parallel, so several shells initialize at once — a
    /// tighter budget was measurably flaky under load. A generous bound only
    /// delays the failure verdict; passing runs still finish the instant the
    /// frame arrives. Mirrors `backend::pty::tests::PTY_TEST_DEADLINE`.
    const SHELL_TEST_DEADLINE: Duration = Duration::from_secs(15);

    fn wait_for<T>(mut take: impl FnMut() -> Option<T>) -> Option<T> {
        let deadline = Instant::now() + SHELL_TEST_DEADLINE;
        while Instant::now() < deadline {
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
        let hub = TerminalHub::spawn(&dir.path().to_string_lossy(), Vec::new());
        let session = hub.connect();

        session.dispatch(ClientMessage::Create { rows: 24, cols: 80 });

        // A create is announced synchronously under the state lock, so this
        // arrives as soon as the worker services the command.
        let created =
            next_matching(&session, |f| created_pane(f).is_some()).expect("no created message");
        let pane = created_pane(&created).unwrap();

        // Drive deterministic output instead of waiting on the interactive
        // prompt, whose timing depends on the user's rc chain and was flaky
        // under parallel load. The marker proves the stream is live end to end.
        let marker = "nightcrow-live";
        session.dispatch(ClientMessage::Input {
            pane,
            data: format!("printf {marker}\n"),
        });

        let output = next_matching(&session, |f| {
            matches!(f, TerminalFrame::Output { pane: p, data }
                if *p == pane && String::from_utf8_lossy(data).contains(marker))
        });
        assert!(output.is_some(), "no output from the shell");
        hub.stop();
    }

    #[test]
    fn the_per_repo_terminal_cap_is_enforced() {
        let dir = tempfile::TempDir::new().unwrap();
        let hub = TerminalHub::spawn(&dir.path().to_string_lossy(), Vec::new());
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
        let hub = TerminalHub::spawn(&dir.path().to_string_lossy(), Vec::new());

        let session = hub.connect();
        assert_eq!(hub.client_count(), 1);
        drop(session);

        assert_eq!(hub.client_count(), 0);
        hub.stop();
    }

    fn created_pane(frame: &TerminalFrame) -> Option<PaneId> {
        let TerminalFrame::Control(json) = frame else {
            return None;
        };
        let value: serde_json::Value = serde_json::from_str(json).ok()?;
        if value["type"] == "created" {
            return value["pane"].as_u64().map(|n| n as PaneId);
        }
        None
    }

    #[test]
    fn a_reconnecting_client_receives_existing_panes_and_scrollback() {
        let dir = tempfile::TempDir::new().unwrap();
        let hub = TerminalHub::spawn(&dir.path().to_string_lossy(), Vec::new());
        let first = hub.connect();

        first.dispatch(ClientMessage::Create { rows: 24, cols: 80 });
        let created =
            next_matching(&first, |f| created_pane(f).is_some()).expect("no created message");
        let pane = created_pane(&created).unwrap();
        // The shell writes a prompt; that is the scrollback a late joiner must
        // get back.
        assert!(
            next_matching(&first, |f| matches!(f, TerminalFrame::Output { .. })).is_some(),
            "no output from the shell"
        );

        // A client that connects afterwards (a refreshed browser) must be told
        // about the pane that already exists and handed its scrollback.
        let second = hub.connect();
        let replayed = next_matching(&second, |f| created_pane(f).is_some())
            .expect("reconnecting client was not told about the existing pane");
        assert_eq!(
            created_pane(&replayed),
            Some(pane),
            "replayed pane id must match the live pane"
        );
        let replay_output =
            next_matching(&second, |f| matches!(f, TerminalFrame::Output { pane: p, .. } if *p == pane));
        assert!(
            replay_output.is_some(),
            "reconnecting client did not receive the scrollback"
        );
        hub.stop();
    }

    #[test]
    fn scrollback_is_bounded_and_keeps_the_most_recent_bytes() {
        let cap = limits::MAX_TERMINAL_SCROLLBACK_BYTES;
        let mut buf = VecDeque::new();
        for _ in 0..(cap / 1000 + 5) {
            push_scrollback(&mut buf, &vec![b'x'; 1000]);
        }
        assert_eq!(buf.len(), cap, "scrollback must be capped");

        // The tail is what restores the visible screen, so the newest bytes must
        // survive eviction.
        push_scrollback(&mut buf, b"TAIL");
        assert_eq!(buf.len(), cap);
        let contents: Vec<u8> = buf.iter().copied().collect();
        assert!(contents.ends_with(b"TAIL"), "newest bytes must be retained");
    }

    #[test]
    fn input_for_an_unknown_pane_is_ignored() {
        // A client racing a pane exit is normal traffic, not an error worth
        // tearing the connection down for.
        let dir = tempfile::TempDir::new().unwrap();
        let hub = TerminalHub::spawn(&dir.path().to_string_lossy(), Vec::new());
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
        let hub = TerminalHub::spawn(&dir.path().to_string_lossy(), Vec::new());
        hub.stop();
        hub.stop();
    }

    #[test]
    fn the_first_connection_spawns_a_startup_terminal() {
        let dir = tempfile::TempDir::new().unwrap();
        let hub = TerminalHub::spawn(
            &dir.path().to_string_lossy(),
            vec!["printf hello".to_string()],
        );
        // Connecting is enough — no client Create is dispatched — to launch the
        // configured startup terminal.
        let session = hub.connect();
        let created = next_matching(
            &session,
            |f| matches!(f, TerminalFrame::Control(json) if json.contains("created")),
        );
        assert!(
            created.is_some(),
            "the startup terminal was not spawned on connect"
        );
        hub.stop();
    }

    #[test]
    fn an_empty_startup_opens_one_shell_on_the_first_connection() {
        let dir = tempfile::TempDir::new().unwrap();
        let hub = TerminalHub::spawn(&dir.path().to_string_lossy(), Vec::new());
        let session = hub.connect();
        let created = next_matching(
            &session,
            |f| matches!(f, TerminalFrame::Control(json) if json.contains("created")),
        );
        assert!(
            created.is_some(),
            "a default shell should open on the first connect"
        );
        hub.stop();
    }
}
