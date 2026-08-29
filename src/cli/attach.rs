use crate::daemon::client::{ConnectError, DaemonClient};
use anyhow::Result;
use std::path::Path;
use std::time::{Duration, Instant};

const DAEMON_READY_TIMEOUT: Duration = Duration::from_secs(20);

/// Start a session when needed, then attach to it.
pub(crate) fn run_attach_detached() -> Result<()> {
    let socket = crate::daemon::socket::default_socket_path()?;
    let log = super::daemon::daemon_output_path()?;
    let client = connect_or_start(&socket, &log)?;
    crate::application::attach::run_attach(client)
}

fn connect_or_start(socket: &Path, log: &Path) -> Result<DaemonClient> {
    let first_unavailable = match DaemonClient::connect_for_attach(socket) {
        Ok(client) => return Ok(client),
        Err(ConnectError::Failed(err)) => return Err(err),
        Err(ConnectError::Unavailable(err)) => err,
    };

    let pid = crate::daemon::detach::respawn_in_background(log)?;
    eprintln!("nightcrow: started a session in the background (pid {pid})");
    eprintln!("nightcrow: its output goes to {}", log.display());
    wait_for_daemon(socket, log, first_unavailable)
}

fn wait_for_daemon(
    socket: &Path,
    log: &Path,
    mut last_unavailable: anyhow::Error,
) -> Result<DaemonClient> {
    let deadline = Instant::now() + DAEMON_READY_TIMEOUT;
    while Instant::now() < deadline {
        match DaemonClient::connect_for_attach(socket) {
            Ok(client) => return Ok(client),
            Err(ConnectError::Failed(err)) => return Err(err),
            Err(ConnectError::Unavailable(err)) => last_unavailable = err,
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    anyhow::bail!(
        "the session did not start within {}s — see {}; last connection error: {}",
        DAEMON_READY_TIMEOUT.as_secs(),
        log.display(),
        last_unavailable
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_unbound_socket_path_does_not_read_as_a_running_daemon() {
        let dir = tempfile::TempDir::new().expect("a temp dir");
        let path = dir.path().join("nightcrow.sock");

        assert!(matches!(
            DaemonClient::connect_for_attach(&path),
            Err(ConnectError::Unavailable(_))
        ));
        std::fs::write(&path, b"").expect("write stale socket stand-in");
        assert!(matches!(
            DaemonClient::connect_for_attach(&path),
            Err(ConnectError::Unavailable(_))
        ));
    }

    #[test]
    fn a_running_daemon_is_handshaken_once_for_the_actual_attach_client() {
        let dir = tempfile::TempDir::new().expect("a temp dir");
        let path = dir.path().join("live.sock");
        let daemon = crate::daemon::socket::DaemonSocket::bind(&path).expect("bind daemon");
        let listener = daemon
            .listener()
            .try_clone()
            .expect("clone daemon listener");
        let server = std::thread::spawn(move || handshake_and_count_clients(listener));

        let _client = connect_or_start(&path, dir.path()).expect("attach succeeds");
        assert_eq!(server.join().expect("server succeeds"), 1);
    }

    fn handshake_and_count_clients(listener: crate::daemon::transport::UnixListener) -> usize {
        use crate::daemon::frame::{Frame, read_frame, write_frame};
        use crate::daemon::protocol::{ClientMessage, ServerMessage, version};
        use std::io::Write;

        listener
            .set_nonblocking(true)
            .expect("make listener nonblocking");
        let deadline = Instant::now() + Duration::from_secs(2);
        let mut accepted = 0;
        let mut handshaken = false;
        while Instant::now() < deadline {
            match listener.accept() {
                Ok((mut stream, _)) => {
                    accepted += 1;
                    stream.set_nonblocking(false).expect("make stream blocking");
                    stream
                        .set_read_timeout(Some(Duration::from_millis(100)))
                        .expect("set handshake timeout");
                    if let Ok(Some(frame)) = read_frame(&mut stream) {
                        let message: ClientMessage =
                            serde_json::from_slice(&frame.payload).expect("decode hello");
                        assert!(matches!(message, ClientMessage::Hello { .. }));
                        let hello = serde_json::to_vec(&ServerMessage::Hello {
                            version: version(),
                            client: 1,
                        })
                        .expect("encode hello");
                        write_frame(&mut stream, &Frame::control(hello)).expect("write hello");
                        stream.flush().expect("flush hello");
                        handshaken = true;
                    }
                    if handshaken {
                        break;
                    }
                }
                Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => {
                    std::thread::sleep(Duration::from_millis(5));
                }
                Err(err) => panic!("accepting attach client: {err}"),
            }
        }
        assert!(
            handshaken,
            "the actual attach client never completed handshake"
        );
        accepted
    }
}
