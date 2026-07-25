use crate::web::common::auth::{Auth, RateLimiter, SessionStore};
use crate::web::common::conn::ConnectionSlot;
use crate::web::protocol::WebInputEvent;
use std::net::TcpListener;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, AtomicUsize};
use std::sync::mpsc::Sender;
use std::thread;

use super::MAX_CONNECTIONS;
use crate::web::server::http_routes::handle_connection;

/// State shared between the accept/handler threads. Holds no `App` reference.
pub(super) struct Shared {
    pub(super) clients: Mutex<Vec<ClientHandle>>,
    pub(super) input_tx: Sender<WebInputEvent>,
    pub(super) auth: Auth,
    pub(super) sessions: SessionStore,
    pub(super) limiter: RateLimiter,
    pub(super) next_id: AtomicU64,
    /// Connections currently held by a handler thread, capped at
    /// [`MAX_CONNECTIONS`]. Owned through [`ConnectionSlot`].
    pub(super) connections: Arc<AtomicUsize>,
}

/// A message queued for one client's handler thread to write to its socket.
pub(super) enum ClientMsg {
    /// Grid dimensions changed (or first frame): tell the browser to resize its
    /// terminal to match before the repaint. Sent as a JSON text frame.
    Resize { cols: u16, rows: u16 },
    /// Encoded ANSI screen bytes. Sent as a binary frame.
    Frame(Vec<u8>),
}

/// A connected browser, as seen by the main loop's broadcast.
pub(super) struct ClientHandle {
    pub(super) id: u64,
    /// Screen updates are pushed here; the client's handler thread writes them
    /// to the socket.
    pub(super) tx: Sender<ClientMsg>,
    /// Set when the client must receive a full repaint on the next frame
    /// (fresh connection or a grid resize).
    pub(super) needs_full: bool,
}

pub(super) fn accept_loop(listener: TcpListener, shared: Arc<Shared>) {
    for stream in listener.incoming() {
        let Ok(stream) = stream else { continue };
        // Refuse over the cap by closing the socket here rather than writing a
        // 503 from the accept loop: a write to a stalled client would block
        // every other connection behind it.
        let Some(slot) = ConnectionSlot::acquire(&shared.connections, MAX_CONNECTIONS) else {
            tracing::debug!(cap = MAX_CONNECTIONS, "web: refusing connection over cap");
            continue;
        };
        let shared = Arc::clone(&shared);
        // One handler thread per connection. A failed spawn drops the closure,
        // and with it the slot; the accept loop keeps serving others.
        let _ = thread::Builder::new()
            .name("nightcrow-web-conn".into())
            .spawn(move || {
                let _slot = slot;
                handle_connection(stream, shared)
            });
    }
}
