//! Synchronous WebSocket/HTTP server for the web mirror.
//!
//! No async runtime: an accept thread spawns one handler thread per connection.
//! Handler threads exchange work with the single-threaded main loop over
//! channels — browser input flows in over an `mpsc` the main loop drains, and
//! encoded screen frames flow out over a per-client channel. The `App` is never
//! shared across threads; only bytes and decoded input events cross the boundary.

mod accept;
mod http_routes;
mod ws;

use crate::web::common::auth::{Auth, RateLimiter, SessionStore};
use crate::web::protocol::{self, WebInputEvent};
use accept::Shared;
use anyhow::{Context, Result};
use ratatui::buffer::Buffer;
use ratatui::layout::Position;
use std::net::{IpAddr, SocketAddr, TcpListener};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, AtomicUsize};
use std::sync::mpsc::{self, Receiver};
use std::thread;
use std::time::Duration;

/// Poll interval for the per-client loop: bounds added output latency while
/// letting the same thread service both socket reads and queued writes.
pub(super) const WS_POLL_TIMEOUT: Duration = Duration::from_millis(10);
/// Live connections allowed at once. Each one costs a thread, so an
/// unbounded accept loop lets anything that can reach the port exhaust the
/// process.
pub(super) const MAX_CONNECTIONS: usize = 64;

/// Handle owned by the main loop. Drop stops nothing (threads live until the
/// process exits, which is the intended lifetime), but it is the sole surface
/// the loop uses to move frames out and input in.
pub struct WebServer {
    shared: Arc<Shared>,
    input_rx: Receiver<WebInputEvent>,
    /// Private baseline of the last broadcast buffer, diffed against each frame.
    /// Main-thread only, so it needs no lock.
    baseline: Option<Buffer>,
    /// Cursor sent with the last broadcast. Tracked separately from `baseline`
    /// because the cursor can move on a frame whose cells are all unchanged
    /// (arrow keys in a shell), which would otherwise send nothing.
    baseline_cursor: Option<Position>,
    addr: SocketAddr,
}

impl WebServer {
    /// Bind and start the server from the `[web_mirror]` config, building the password
    /// verifier from either `hashed_password` or `password`.
    pub fn start_from_config(web: &crate::config::WebMirrorConfig) -> Result<Self> {
        let auth = if let Some(hash) = web.hashed_password.as_deref() {
            Auth::from_hashed(hash)?
        } else if let Some(password) = web.password.as_deref().filter(|p| !p.is_empty()) {
            Auth::from_plaintext(password)?
        } else {
            anyhow::bail!("web_mirror is enabled but no password or hashed_password is configured");
        };
        let bind: IpAddr = web
            .bind
            .parse()
            .with_context(|| format!("web_mirror.bind {:?} is not a valid IP address", web.bind))?;
        Self::start(bind, web.port, auth)
    }

    /// Bind the server and start accepting connections in the background.
    fn start(bind: IpAddr, port: u16, auth: Auth) -> Result<Self> {
        let listener = TcpListener::bind((bind, port))
            .with_context(|| format!("binding web server to {bind}:{port}"))?;
        let addr = listener
            .local_addr()
            .unwrap_or_else(|_| SocketAddr::new(bind, port));

        let (input_tx, input_rx) = mpsc::channel();
        let shared = Arc::new(Shared {
            clients: std::sync::Mutex::new(Vec::new()),
            input_tx,
            auth,
            sessions: SessionStore::new(),
            limiter: RateLimiter::new(),
            next_id: AtomicU64::new(0),
            connections: Arc::new(AtomicUsize::new(0)),
        });

        let accept_shared = Arc::clone(&shared);
        thread::Builder::new()
            .name("nightcrow-web-accept".into())
            .spawn(move || accept::accept_loop(listener, accept_shared))
            .context("spawning web accept thread")?;

        Ok(Self {
            shared,
            input_rx,
            baseline: None,
            baseline_cursor: None,
            addr,
        })
    }

    /// The address the server actually bound (port may differ if 0 was given).
    pub fn addr(&self) -> SocketAddr {
        self.addr
    }

    /// Drain all input events received from browsers since the last call.
    pub fn drain_input(&self) -> Vec<WebInputEvent> {
        let mut out = Vec::new();
        while let Ok(ev) = self.input_rx.try_recv() {
            out.push(ev);
        }
        out
    }

    /// Send the frame needed to bring every connected client up to `current`.
    ///
    /// New clients (and all clients after a grid resize) get a full repaint;
    /// the rest get the incremental cell diff against the last broadcast. A
    /// no-op when no clients are connected — the baseline is dropped so the
    /// next client to connect receives a full frame.
    ///
    /// `cursor` is the cell `ui::draw` placed the terminal cursor on, which is
    /// not part of `current` and must be replayed explicitly.
    pub fn broadcast(&mut self, current: &Buffer, cursor: Option<Position>) {
        // Lock first and bail on no clients so a disconnected session never
        // pays for frame encoding. The baseline is dropped so the next client
        // to connect is treated as needing a full repaint.
        let mut clients = match self.shared.clients.lock() {
            Ok(c) => c,
            Err(_) => return,
        };
        if clients.is_empty() {
            drop(clients);
            self.baseline = None;
            self.baseline_cursor = None;
            return;
        }

        let area_changed = self
            .baseline
            .as_ref()
            .is_some_and(|prev| prev.area() != current.area());
        let cursor_bytes = protocol::encode_cursor(cursor);
        let cursor_moved = cursor != self.baseline_cursor;
        // Incremental diff against our own baseline (a different field from the
        // locked `clients`, so the borrows are disjoint). A frame with no cell
        // changes still ships when the cursor alone moved.
        let update_bytes = if area_changed {
            None
        } else {
            self.baseline
                .as_ref()
                .map(|prev| protocol::encode_update(prev, current))
                .filter(|cells| !cells.is_empty() || cursor_moved)
                .map(|mut bytes| {
                    bytes.extend_from_slice(&cursor_bytes);
                    bytes
                })
        };
        let mut full_bytes: Option<Vec<u8>> = None;

        let area = current.area();
        let (cols, rows) = (area.width, area.height);
        let mut dead = Vec::new();
        for client in clients.iter_mut() {
            let needs_full = client.needs_full || area_changed;
            let result = if needs_full {
                let bytes = full_bytes.get_or_insert_with(|| {
                    let mut bytes = protocol::encode_full_frame(current);
                    bytes.extend_from_slice(&cursor_bytes);
                    bytes
                });
                client.needs_full = false;
                // The browser must resize its terminal to the grid before the
                // repaint lands, so the frame's absolute cursor moves address
                // the right cells.
                client
                    .tx
                    .send(accept::ClientMsg::Resize { cols, rows })
                    .and_then(|()| client.tx.send(accept::ClientMsg::Frame(bytes.clone())))
            } else if let Some(bytes) = update_bytes.as_ref() {
                client.tx.send(accept::ClientMsg::Frame(bytes.clone()))
            } else {
                Ok(())
            };
            if result.is_err() {
                dead.push(client.id);
            }
        }
        if !dead.is_empty() {
            clients.retain(|c| !dead.contains(&c.id));
        }
        drop(clients);
        self.baseline = Some(current.clone());
        self.baseline_cursor = cursor;
    }
}

#[cfg(test)]
mod tests;
