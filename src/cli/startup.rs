use crate::daemon::client::{ConnectError, DaemonClient};
use anyhow::{Context, Result};
use std::path::Path;
use std::process::Child;
use std::time::{Duration, Instant};

const DAEMON_READY_TIMEOUT: Duration = Duration::from_secs(20);
const RETRY_INTERVAL: Duration = Duration::from_millis(50);

pub(super) fn wait_for_daemon(
    child: &mut Child,
    socket: &Path,
    log: &Path,
    last_unavailable: anyhow::Error,
) -> Result<DaemonClient> {
    wait_for_daemon_with_timeout(child, socket, log, last_unavailable, DAEMON_READY_TIMEOUT)
}

fn wait_for_daemon_with_timeout(
    child: &mut Child,
    socket: &Path,
    log: &Path,
    mut last_unavailable: anyhow::Error,
    ready_timeout: Duration,
) -> Result<DaemonClient> {
    let deadline = Instant::now() + ready_timeout;
    while Instant::now() < deadline {
        match DaemonClient::connect_for_attach(socket) {
            Ok(client) => return Ok(client),
            Err(ConnectError::Failed(err)) => return Err(err),
            Err(ConnectError::Unavailable(err)) => {
                let pid = child.id();
                if let Some(status) = child
                    .try_wait()
                    .context("checking whether the background daemon exited")?
                {
                    anyhow::bail!(
                        "session daemon process {pid} exited ({status}) before it was ready — see {}",
                        log.display()
                    );
                }
                last_unavailable = err;
            }
        }
        std::thread::sleep(RETRY_INTERVAL);
    }

    anyhow::bail!(
        "the session did not start within {}s — see {}; last connection error: {}",
        ready_timeout.as_secs(),
        log.display(),
        last_unavailable
    )
}

#[cfg(test)]
#[path = "startup_tests.rs"]
mod tests;
