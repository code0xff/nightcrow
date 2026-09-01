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
use serde::Deserialize;
use std::io::Write;
use std::path::Path;
use std::time::Duration;

/// The status shape emitted before the web and attach endpoints were split.
/// It is intentionally private and only used to turn a precise, same-version
/// compatibility failure into an actionable error at the one-shot boundary.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LegacyStatusResponse {
    #[serde(rename = "type")]
    message_type: String,
    status: LegacyDaemonStatus,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LegacyDaemonStatus {
    #[serde(rename = "pid")]
    _pid: u32,
    version: String,
    #[serde(rename = "started_at_unix_ms")]
    _started_at_unix_ms: Result<u64, super::protocol::StatusUnavailable>,
    #[serde(rename = "uptime_ms")]
    _uptime_ms: u64,
    #[serde(rename = "endpoint")]
    _endpoint: Result<String, super::protocol::StatusUnavailable>,
    #[serde(rename = "repositories")]
    _repositories: Vec<super::protocol::RepositoryStatus>,
    #[serde(rename = "attached_clients")]
    _attached_clients: Vec<u64>,
}

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
    match serde_json::from_slice(&frame.payload) {
        Ok(response) => Ok(response),
        Err(error) => {
            if matches!(request, ClientMessage::Status {})
                && is_legacy_status_response(&frame.payload)
            {
                bail!(
                    "protocol incompatibility: daemon status response uses the legacy endpoint field; restart the daemon after updating nightcrow"
                );
            }
            Err(error).context("protocol error: malformed daemon response JSON")
        }
    }
}

fn is_legacy_status_response(payload: &[u8]) -> bool {
    let Ok(response) = serde_json::from_slice::<LegacyStatusResponse>(payload) else {
        return false;
    };
    response.message_type == "status" && response.status.version == super::protocol::version()
}

#[cfg(test)]
#[path = "one_shot_tests.rs"]
mod tests;
