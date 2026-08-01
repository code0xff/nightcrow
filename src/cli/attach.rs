use anyhow::Result;
use std::path::Path;
use std::time::{Duration, Instant};

const DAEMON_READY_TIMEOUT: Duration = Duration::from_secs(20);

/// Start a session when needed, then attach to it.
pub(crate) fn run_attach_detached() -> Result<()> {
    let socket = crate::daemon::socket::default_socket_path()?;
    if daemon_accepts(&socket) {
        return crate::application::attach::run_attach();
    }

    let log = super::daemon::daemon_output_path()?;
    let pid = crate::daemon::detach::respawn_in_background(&log)?;
    eprintln!("nightcrow: started a session in the background (pid {pid})");
    eprintln!("nightcrow: its output goes to {}", log.display());
    wait_for_daemon(&socket, &log)?;
    crate::application::attach::run_attach()
}

fn daemon_accepts(socket: &Path) -> bool {
    crate::daemon::transport::UnixStream::connect(socket).is_ok()
}

fn wait_for_daemon(socket: &Path, log: &Path) -> Result<()> {
    let deadline = Instant::now() + DAEMON_READY_TIMEOUT;
    while Instant::now() < deadline {
        if daemon_accepts(socket) {
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    anyhow::bail!(
        "the session did not start within {}s — see {}",
        DAEMON_READY_TIMEOUT.as_secs(),
        log.display()
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_unbound_socket_path_does_not_read_as_a_running_daemon() {
        let dir = tempfile::TempDir::new().expect("a temp dir");
        let path = dir.path().join("nightcrow.sock");

        assert!(!daemon_accepts(&path));
        std::fs::write(&path, b"").expect("write stale socket stand-in");
        assert!(!daemon_accepts(&path));
    }

    #[test]
    fn a_bound_socket_reads_as_a_running_daemon() {
        let dir = tempfile::TempDir::new().expect("a temp dir");
        let path = dir.path().join("live.sock");
        let _listener =
            crate::daemon::transport::UnixListener::bind(&path).expect("bind probe socket");

        assert!(daemon_accepts(&path));
        wait_for_daemon(&path, dir.path()).expect("already accepting");
    }
}
