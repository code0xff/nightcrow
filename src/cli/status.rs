use anyhow::{Result, bail};
use std::path::PathBuf;
use std::time::Duration;

use crate::daemon::one_shot::request;
use crate::daemon::protocol::{
    ClientMessage, DaemonStatus, RepositoryStatus, ServerMessage, version,
};

#[path = "status_render.rs"]
mod status_render;

use status_render::render_status;

const STATUS_TIMEOUT: Duration = Duration::from_secs(5);

/// Query the daemon without creating an attach client or terminal subscription.
pub(crate) fn run_status(socket: Option<PathBuf>) -> Result<()> {
    let path = resolve_socket_path(socket, crate::daemon::socket::default_socket_path)?;
    let status = query_status(&path)?;
    println!("{}", render_status(&status));
    Ok(())
}

fn resolve_socket_path<F>(socket: Option<PathBuf>, default_path: F) -> Result<PathBuf>
where
    F: FnOnce() -> Result<PathBuf>,
{
    match socket {
        Some(path) => Ok(path),
        None => default_path(),
    }
}

fn query_status(path: &std::path::Path) -> Result<DaemonStatus> {
    let response = match request(path, &ClientMessage::Status {}, STATUS_TIMEOUT) {
        Ok(response) => response,
        Err(error) if socket_unavailable(&error) => {
            bail!(
                "daemon unavailable at {}: no socket or listener is running; start a session with `nightcrow -d`",
                path.display()
            )
        }
        Err(error) => return Err(error),
    };
    let status = decode_status(response)?;
    validate_status(&status)?;
    Ok(status)
}

fn socket_unavailable(error: &anyhow::Error) -> bool {
    error.downcast_ref::<std::io::Error>().is_some_and(|error| {
        matches!(
            error.kind(),
            std::io::ErrorKind::NotFound | std::io::ErrorKind::ConnectionRefused
        )
    })
}

fn decode_status(response: ServerMessage) -> Result<DaemonStatus> {
    match response {
        ServerMessage::Status { status } => {
            let expected = version();
            if status.version != expected {
                bail!(
                    "version mismatch: daemon reports {}, this client expects {}",
                    status_render::display_text(&status.version),
                    expected
                );
            }
            Ok(status)
        }
        ServerMessage::Error { message } => {
            bail!("protocol error: daemon rejected the status request: {message}")
        }
        other => bail!("protocol error: unexpected response to status request: {other:?}"),
    }
}

fn validate_status(status: &DaemonStatus) -> Result<()> {
    if status.pid == 0 {
        bail!("protocol error: malformed status response: PID is zero");
    }
    if status.web_endpoint.is_empty() {
        bail!("protocol error: malformed status response: web endpoint is empty");
    }
    if let Ok(endpoint) = &status.attach_endpoint
        && endpoint.is_empty()
    {
        bail!("protocol error: malformed status response: attach endpoint is empty");
    }
    let mut client_ids = status.attached_clients.clone();
    client_ids.sort_unstable();
    if client_ids.windows(2).any(|ids| ids[0] == ids[1]) {
        bail!("protocol error: malformed status response: duplicate client id");
    }
    let mut repo_ids = Vec::with_capacity(status.repositories.len());
    for repo in &status.repositories {
        validate_repository(repo)?;
        repo_ids.push(&repo.id);
    }
    repo_ids.sort_unstable();
    if repo_ids.windows(2).any(|ids| ids[0] == ids[1]) {
        bail!("protocol error: malformed status response: duplicate repository id");
    }
    Ok(())
}

fn validate_repository(repo: &RepositoryStatus) -> Result<()> {
    if repo.id.is_empty() || repo.path.is_empty() {
        bail!("protocol error: malformed status response: repository identity is empty");
    }
    if repo.pane_count != repo.panes.len() {
        bail!(
            "protocol error: malformed status response: repository {} pane count disagrees with pane ids",
            status_render::display_text(&repo.id)
        );
    }
    let mut panes = repo.panes.clone();
    panes.sort_unstable();
    if panes.windows(2).any(|ids| ids[0] == ids[1]) {
        bail!(
            "protocol error: malformed status response: repository {} has duplicate pane id",
            status_render::display_text(&repo.id)
        );
    }
    Ok(())
}

#[cfg(test)]
#[path = "status_tests.rs"]
mod tests;
