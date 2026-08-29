use anyhow::{Context, Result};
use std::io::Read;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use crate::daemon::frame::{FrameKind, read_frame};
use crate::daemon::one_shot::{connect, send_request};
use crate::daemon::protocol::ServerMessage;

// The daemon's cleanup normally takes milliseconds, but a configured plugin may
// take up to 200 ms per host before it is force-killed. Keep room for the
// bounded cleanup of a full configured session while still rejecting a lost
// request instead of waiting forever.
const SHUTDOWN_ACK_TIMEOUT: Duration = Duration::from_secs(20);

/// Send a graceful shutdown request to a running daemon.
pub(crate) fn run_stop(socket: Option<PathBuf>) -> Result<()> {
    let path = match socket {
        Some(path) => path,
        None => crate::daemon::socket::default_socket_path()?,
    };
    if !path.exists() {
        anyhow::bail!(
            "no daemon socket at {} — is a nightcrow daemon running?",
            path.display()
        );
    }

    let mut stream = connect(&path).with_context(|| {
        format!(
            "could not connect to the daemon socket at {} — the daemon may have stopped",
            path.display()
        )
    })?;
    send_request(
        &mut stream,
        &crate::daemon::protocol::ClientMessage::Shutdown,
    )
    .context("sending the shutdown request")?;
    stream
        .set_read_timeout(Some(SHUTDOWN_ACK_TIMEOUT))
        .context("setting the shutdown acknowledgment timeout")?;

    wait_for_shutdown_ack(&mut stream, Instant::now() + SHUTDOWN_ACK_TIMEOUT)?;

    println!("nightcrow: daemon is shutting down");
    Ok(())
}

/// Consume any frames until the daemon closes this one-shot connection.
///
/// A current daemon closes immediately after accepting the one-shot request;
/// older daemons may have queued session frames first. Only EOF, or a
/// reset/abort while the daemon is closing, proves shutdown reached its exit
/// path.
fn wait_for_shutdown_ack<R: Read>(reader: &mut R, deadline: Instant) -> Result<()> {
    loop {
        if Instant::now() >= deadline {
            anyhow::bail!("timed out waiting for the daemon to acknowledge the shutdown");
        }
        let frame = match read_frame(reader) {
            Ok(frame) => frame,
            Err(err) if expected_disconnect(&err) => return Ok(()),
            Err(err) => {
                return Err(err).context("waiting for the daemon to acknowledge the shutdown");
            }
        };
        let Some(frame) = frame else {
            return Ok(());
        };

        if frame.kind != FrameKind::Control {
            continue;
        }
        let message: ServerMessage = serde_json::from_slice(&frame.payload)
            .context("decoding a daemon response while waiting for shutdown")?;
        if let ServerMessage::Error { message } = message {
            anyhow::bail!("daemon rejected the shutdown request: {message}");
        }
    }
}

fn expected_disconnect(err: &anyhow::Error) -> bool {
    err.downcast_ref::<std::io::Error>().is_some_and(|error| {
        matches!(
            error.kind(),
            std::io::ErrorKind::ConnectionReset
                | std::io::ErrorKind::ConnectionAborted
                | std::io::ErrorKind::UnexpectedEof
        )
    })
}

#[cfg(test)]
#[path = "stop_tests.rs"]
mod tests;
