//! The short request path used before a daemon connection becomes stateful.
//!
//! Status is deliberately not a [`DaemonClient`]: it writes one control frame,
//! reads one control response, and lets the daemon close the connection. Stop
//! shares only this small connect/write seam; its EOF acknowledgment remains
//! specialized in the CLI.

use super::frame::{Frame, FrameKind, read_frame, write_frame};
use super::protocol::{ClientMessage, ServerMessage};
use super::transport::UnixStream;
use anyhow::{Context, Result, bail};
use std::io::Write;
use std::path::Path;
use std::time::Duration;

pub(crate) fn connect(path: &Path) -> std::io::Result<UnixStream> {
    UnixStream::connect(path)
}

pub(crate) fn send_request(stream: &mut UnixStream, request: &ClientMessage) -> Result<()> {
    let json = serde_json::to_vec(request).context("encoding a daemon request")?;
    write_frame(stream, &Frame::control(json)).context("sending a daemon request")?;
    stream.flush().context("flushing a daemon request")
}

/// Send one request and read exactly one framed server response.
pub(crate) fn request(
    path: &Path,
    request: &ClientMessage,
    timeout: Duration,
) -> Result<ServerMessage> {
    let mut stream = connect(path)
        .with_context(|| format!("connecting to the daemon socket at {}", path.display()))?;
    send_request(&mut stream, request)?;
    stream
        .set_read_timeout(Some(timeout))
        .context("setting the one-shot daemon response timeout")?;
    let frame = read_frame(&mut stream)
        .context("wire error while reading the daemon response")?
        .ok_or_else(|| anyhow::anyhow!("wire error: daemon closed before sending a response"))?;
    if frame.kind != FrameKind::Control {
        bail!(
            "wire error: expected a control response, got {:?}",
            frame.kind
        );
    }
    serde_json::from_slice(&frame.payload).context("protocol error: malformed daemon response JSON")
}

#[cfg(test)]
#[path = "one_shot_tests.rs"]
mod tests;
