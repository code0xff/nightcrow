use super::Session;
use crate::daemon::frame::{FrameKind, encode_server, read_frame, write_frame};
use crate::daemon::protocol::{ClientMessage, ServerMessage};
use crate::daemon::transport::UnixStream;
use anyhow::Result;
use std::io::Write;

/// Read the only frame allowed before attachment. `Some(version)` admits the
/// connection to the stateful attach path; every other outcome is complete.
pub(super) fn read(stream: &mut UnixStream, session: &Session) -> Result<Option<String>> {
    let Some(frame) = read_frame(stream)? else {
        return Ok(None);
    };
    if frame.kind != FrameKind::Control {
        reply_and_close(
            stream,
            error("first frame must be hello, status, or shutdown"),
        )?;
        return Ok(None);
    }
    let message = match serde_json::from_slice::<ClientMessage>(&frame.payload) {
        Ok(message) => message,
        Err(err) => {
            reply_and_close(stream, error(&format!("unreadable first request: {err}")))?;
            return Ok(None);
        }
    };
    match message {
        ClientMessage::Hello { version } => Ok(Some(version)),
        ClientMessage::Status {} => {
            let status = session.metadata.snapshot(session);
            reply_and_close(stream, ServerMessage::Status { status })?;
            Ok(None)
        }
        ClientMessage::Shutdown => {
            // Stop is also one-shot: it must keep working with the handshake
            // now required by stateful attach, without registering a client.
            let _ = session
                .shutdown_tx
                .send(crate::platform::signals::Shutdown::Terminate);
            Ok(None)
        }
        _ => {
            reply_and_close(
                stream,
                error("first request must be hello, status, or shutdown"),
            )?;
            Ok(None)
        }
    }
}

fn reply_and_close(stream: &mut UnixStream, message: ServerMessage) -> Result<()> {
    let frame = encode_server(
        &message,
        "pre-attach reply",
        "pre-attach reply could not be encoded",
    );
    write_frame(stream, &frame)?;
    stream.flush()?;
    Ok(())
}

fn error(message: &str) -> ServerMessage {
    ServerMessage::Error {
        message: message.to_string(),
    }
}
