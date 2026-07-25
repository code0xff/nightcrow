use crate::web::common::conn;
use crate::web::common::http::RequestHead;
use crate::web::protocol;
use std::net::TcpStream;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::sync::mpsc::Receiver;
use tungstenite::{Message, WebSocket};

use super::WS_POLL_TIMEOUT;
use super::accept::{ClientHandle, ClientMsg, Shared};

/// Complete the WebSocket handshake manually (the request head was already
/// consumed for routing/auth), then run the per-client read/write loop.
pub(super) fn serve_websocket(stream: TcpStream, head: &RequestHead, shared: Arc<Shared>) {
    if let Some(ws) = conn::websocket_handshake(stream, head) {
        run_client(ws, shared);
    }
}

fn run_client(mut ws: WebSocket<TcpStream>, shared: Arc<Shared>) {
    let (tx, rx) = std::sync::mpsc::channel::<ClientMsg>();
    let id = shared.next_id.fetch_add(1, Ordering::Relaxed);
    if let Ok(mut clients) = shared.clients.lock() {
        clients.push(ClientHandle {
            id,
            tx,
            needs_full: true,
        });
    }
    // A read timeout turns the blocking read into a poll so the same thread can
    // also flush queued output frames.
    ws.get_ref().set_read_timeout(Some(WS_POLL_TIMEOUT)).ok();

    loop {
        if !pump_writes(&mut ws, &rx) {
            break;
        }
        if !pump_read(&mut ws, &shared) {
            break;
        }
    }

    if let Ok(mut clients) = shared.clients.lock() {
        clients.retain(|c| c.id != id);
    }
}

/// Drain and send any queued output messages. Returns false on a write error
/// (client gone).
fn pump_writes(ws: &mut WebSocket<TcpStream>, rx: &Receiver<ClientMsg>) -> bool {
    while let Ok(msg) = rx.try_recv() {
        let written = match msg {
            ClientMsg::Resize { cols, rows } => ws.write(Message::text(format!(
                r#"{{"t":"resize","cols":{cols},"rows":{rows}}}"#
            ))),
            ClientMsg::Frame(bytes) => ws.write(Message::binary(bytes)),
        };
        if written.is_err() {
            return false;
        }
    }
    ws.flush().is_ok()
}

/// Read at most one input message. Returns false when the connection should
/// close; a read timeout (no data yet) returns true so the loop continues.
fn pump_read(ws: &mut WebSocket<TcpStream>, shared: &Shared) -> bool {
    match ws.read() {
        Ok(msg) => {
            if msg.is_close() {
                return false;
            }
            if msg.is_text() || msg.is_binary() {
                let data = msg.into_data();
                if let Ok(text) = std::str::from_utf8(data.as_ref()) {
                    dispatch_input(text, shared);
                }
            }
            true
        }
        Err(tungstenite::Error::Io(e))
            if matches!(
                e.kind(),
                std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
            ) =>
        {
            // Poll timeout: no message this round.
            true
        }
        Err(_) => false,
    }
}

fn dispatch_input(text: &str, shared: &Shared) {
    if protocol::ensure_input_size(text.len()).is_err() {
        tracing::debug!("web: dropping oversized input message");
        return;
    }
    match protocol::decode_input(text) {
        Ok(Some(event)) => {
            let _ = shared.input_tx.send(event);
        }
        Ok(None) => {}
        Err(err) => tracing::debug!(%err, "web: dropping undecodable input"),
    }
}
