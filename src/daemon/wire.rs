//! Speaking on an attach socket: writing requests, and sorting what comes back.
//!
//! Below both halves of the client. The connection is multiplexed — the session
//! link asks about repositories while every open repository's terminal backend
//! sends input on the same socket — so writing is locked, and reading decides
//! which of them a frame belongs to before anyone waits on it.

use super::frame::{Frame, FrameKind, read_frame, write_frame};
use super::protocol::{ClientMessage, ServerMessage, TerminalOutput};
use super::terminal_link::{TerminalMessage, TerminalRouter};
use super::transport::UnixStream;
use crate::session::terminal::frame::ServerMessage as HubServerMessage;
use anyhow::{Context, Result};
use std::io::Write;
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};

/// The write half of an attach socket. Shared and locked because two kinds of
/// caller send on it; a frame is written under the lock, so two writers cannot
/// interleave halves of one message.
pub(super) type Writer = Arc<Mutex<UnixStream>>;

/// Write one request. Holds the connection lock for the whole frame.
pub(super) fn send(out: &Writer, message: &ClientMessage) -> Result<()> {
    let json = serde_json::to_vec(message).context("encoding a daemon request")?;
    let mut out = out.lock().expect("daemon connection poisoned");
    write_frame(&mut *out, &Frame::control(json))?;
    out.flush().context("flushing a daemon request")
}

/// What one frame off the connection turned out to be.
pub(super) enum Incoming {
    /// A message for the caller's own queue.
    Control(ServerMessage),
    /// Terminal traffic, already filed with the router.
    Routed,
}

/// Read frames and route them until the daemon closes.
///
/// A decode failure ends the loop rather than being skipped: the two sides ship
/// in one binary, so a frame this client cannot read means the stream is no
/// longer what it claims to be, and going on would deliver a session state built
/// from whatever survived.
pub(super) fn pump(
    reader: &mut UnixStream,
    terminals: &TerminalRouter,
    tx: &Sender<ServerMessage>,
) {
    loop {
        match read_routed(reader, terminals) {
            // A clean end of stream: the daemon stopped or the client detached.
            Ok(None) => return,
            Ok(Some(Incoming::Routed)) => {}
            Ok(Some(Incoming::Control(message))) => {
                // Ends when the receiver is dropped, i.e. the client is gone.
                if tx.send(message).is_err() {
                    return;
                }
            }
            // A read that timed out is not a disconnect. A quiet session is the
            // normal state, and the handshake's timeout can outlive the
            // handshake (macOS refuses to clear the option once the peer has
            // gone). Inventing a disconnect out of an idle session is the one
            // failure this whole shape exists to avoid.
            Err(err) if timed_out(&err) => {}
            Err(err) => {
                tracing::warn!(%err, "daemon connection ended");
                return;
            }
        }
    }
}

/// Whether a failed read was only the socket's timeout expiring.
fn timed_out(err: &anyhow::Error) -> bool {
    err.downcast_ref::<std::io::Error>().is_some_and(|err| {
        matches!(
            err.kind(),
            std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
        )
    })
}

/// Read one frame, filing terminal traffic with `terminals` and handing back
/// anything else. `None` at a clean end of stream.
pub(super) fn read_routed(
    reader: &mut UnixStream,
    terminals: &TerminalRouter,
) -> Result<Option<Incoming>> {
    let Some(frame) = read_frame(reader)? else {
        return Ok(None);
    };
    if frame.kind == FrameKind::Terminal {
        let output = TerminalOutput::decode(&frame.payload)
            .context("decoding pane output from the daemon")?;
        terminals.deliver(
            &output.repo,
            TerminalMessage::Output {
                pane: output.pane,
                data: output.data,
            },
        );
        return Ok(Some(Incoming::Routed));
    }
    let message: ServerMessage =
        serde_json::from_slice(&frame.payload).context("decoding a message from the daemon")?;
    // A terminal event belongs to one repository's inbox — except a refusal,
    // which is not about a pane and has to reach the tab that shows notices.
    if let ServerMessage::Terminal { repo, event } = &message
        && !matches!(event, HubServerMessage::Error { .. })
    {
        terminals.deliver(repo, TerminalMessage::Event(event.clone()));
        return Ok(Some(Incoming::Routed));
    }
    Ok(Some(Incoming::Control(message)))
}
