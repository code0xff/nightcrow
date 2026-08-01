use anyhow::{Context, Result};
use std::io::Write;
use std::path::PathBuf;

use crate::daemon::frame::{Frame, read_frame, write_frame};
use crate::daemon::protocol::ClientMessage;
use crate::daemon::transport::UnixStream;

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

    let mut stream = UnixStream::connect(&path).with_context(|| {
        format!(
            "could not connect to the daemon socket at {} — the daemon may have stopped",
            path.display()
        )
    })?;
    let json =
        serde_json::to_vec(&ClientMessage::Shutdown).context("encoding the shutdown request")?;
    write_frame(&mut stream, &Frame::control(json)).context("sending the shutdown request")?;
    stream.flush().context("flushing the shutdown request")?;

    // Closing the connection is the daemon's acknowledgment. A reset is also
    // expected when shutdown wins the race with this read.
    if let Err(err) = read_frame(&mut stream) {
        let expected_disconnect = err.downcast_ref::<std::io::Error>().is_some_and(|error| {
            matches!(
                error.kind(),
                std::io::ErrorKind::ConnectionReset
                    | std::io::ErrorKind::ConnectionAborted
                    | std::io::ErrorKind::UnexpectedEof
            )
        });
        if !expected_disconnect {
            return Err(err).context("waiting for the daemon to acknowledge the shutdown");
        }
    }

    println!("nightcrow: daemon is shutting down");
    Ok(())
}
