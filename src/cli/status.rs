use std::fmt;
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

/// Exit codes for the status command's failure classes.
///
/// 0 is success; the classes below are stable so scripts can branch on them
/// without parsing stderr. Codes avoid the shell conventions 1 (generic) and
/// 2 (usage/parse) as well as 130+ (signals).
pub mod exit_code {
    pub(crate) const DAEMON_STOPPED: u8 = 3;
    pub(crate) const RESPONSE_TIMEOUT: u8 = 4;
    pub(crate) const PROTOCOL_ERROR: u8 = 5;
}

/// The status command's failure classes, each with a dedicated exit code.
#[derive(Debug)]
pub(crate) enum StatusError {
    /// No socket or listener: the daemon is not running.
    Stopped { path: PathBuf },
    /// The daemon accepted the connection but did not answer in time.
    Timeout { path: PathBuf },
    /// The daemon answered, but the response violated the status contract.
    Protocol(String),
}

impl StatusError {
    pub(crate) fn exit_code(&self) -> i32 {
        match self {
            StatusError::Stopped { .. } => exit_code::DAEMON_STOPPED as i32,
            StatusError::Timeout { .. } => exit_code::RESPONSE_TIMEOUT as i32,
            StatusError::Protocol(_) => exit_code::PROTOCOL_ERROR as i32,
        }
    }
}

impl fmt::Display for StatusError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            StatusError::Stopped { path } => write!(
                f,
                "daemon stopped: no socket or listener at {}; start a session with `nightcrow -d`",
                path.display()
            ),
            StatusError::Timeout { path } => write!(
                f,
                "response timeout: daemon at {} accepted the connection but did not answer within {}s",
                path.display(),
                STATUS_TIMEOUT.as_secs()
            ),
            StatusError::Protocol(message) => write!(f, "protocol error: {message}"),
        }
    }
}

impl std::error::Error for StatusError {}

/// Query the daemon without creating an attach client or terminal subscription.
pub(crate) fn run_status(socket: Option<PathBuf>) -> Result<(), StatusError> {
    let path = resolve_socket_path(socket, crate::daemon::socket::default_socket_path)?;
    let status = query_status(&path)?;
    println!("{}", render_status(&status));
    Ok(())
}

fn resolve_socket_path<F>(socket: Option<PathBuf>, default_path: F) -> Result<PathBuf, StatusError>
where
    F: FnOnce() -> anyhow::Result<PathBuf>,
{
    match socket {
        Some(path) => Ok(path),
        None => default_path().map_err(|error| {
            StatusError::Protocol(format!(
                "could not resolve the default socket path: {error:#}"
            ))
        }),
    }
}

fn query_status(path: &std::path::Path) -> Result<DaemonStatus, StatusError> {
    let response = request(path, &ClientMessage::Status {}, STATUS_TIMEOUT)
        .map_err(|error| classify_request_error(path, &error))?;
    let status = decode_status(response).map_err(StatusError::Protocol)?;
    validate_status(&status).map_err(StatusError::Protocol)?;
    Ok(status)
}

/// Split a one-shot request failure into stopped / timeout / protocol classes.
///
/// The one-shot seam reports connect failures as `io::Error`s attached to the
/// anyhow chain; read failures past an established connection surface the same
/// way, so a read timeout is only a `Timeout` when the connect context is
/// absent.
fn classify_request_error(path: &std::path::Path, error: &anyhow::Error) -> StatusError {
    if socket_unavailable(error) {
        return StatusError::Stopped {
            path: path.to_path_buf(),
        };
    }
    if read_timed_out(error) {
        return StatusError::Timeout {
            path: path.to_path_buf(),
        };
    }
    StatusError::Protocol(format!("{error:#}"))
}

fn socket_unavailable(error: &anyhow::Error) -> bool {
    error.downcast_ref::<std::io::Error>().is_some_and(|error| {
        matches!(
            error.kind(),
            std::io::ErrorKind::NotFound | std::io::ErrorKind::ConnectionRefused
        )
    })
}

fn read_timed_out(error: &anyhow::Error) -> bool {
    error.downcast_ref::<std::io::Error>().is_some_and(|error| {
        matches!(
            error.kind(),
            std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
        )
    })
}

fn decode_status(response: ServerMessage) -> Result<DaemonStatus, String> {
    match response {
        ServerMessage::Status { status } => {
            let expected = version();
            if status.version != expected {
                return Err(format!(
                    "version mismatch: daemon reports {}, this client expects {}",
                    status_render::display_text(&status.version),
                    expected
                ));
            }
            Ok(status)
        }
        ServerMessage::Error { message } => {
            Err(format!("daemon rejected the status request: {message}"))
        }
        other => Err(format!("unexpected response to status request: {other:?}")),
    }
}

fn validate_status(status: &DaemonStatus) -> Result<(), String> {
    if status.pid == 0 {
        return Err("malformed status response: PID is zero".into());
    }
    if status.web_endpoint.is_empty() {
        return Err("malformed status response: web endpoint is empty".into());
    }
    if let Ok(endpoint) = &status.attach_endpoint
        && endpoint.is_empty()
    {
        return Err("malformed status response: attach endpoint is empty".into());
    }
    let mut client_ids = status.attached_clients.clone();
    client_ids.sort_unstable();
    if client_ids.windows(2).any(|ids| ids[0] == ids[1]) {
        return Err("malformed status response: duplicate client id".into());
    }
    let mut repo_ids = Vec::with_capacity(status.repositories.len());
    for repo in &status.repositories {
        validate_repository(repo)?;
        repo_ids.push(&repo.id);
    }
    repo_ids.sort_unstable();
    if repo_ids.windows(2).any(|ids| ids[0] == ids[1]) {
        return Err("malformed status response: duplicate repository id".into());
    }
    Ok(())
}

fn validate_repository(repo: &RepositoryStatus) -> Result<(), String> {
    if repo.id.is_empty() || repo.path.is_empty() {
        return Err("malformed status response: repository identity is empty".into());
    }
    if repo.pane_count != repo.panes.len() {
        return Err(format!(
            "malformed status response: repository {} pane count disagrees with pane ids",
            status_render::display_text(&repo.id)
        ));
    }
    let mut panes = repo.panes.clone();
    panes.sort_unstable();
    if panes.windows(2).any(|ids| ids[0] == ids[1]) {
        return Err(format!(
            "malformed status response: repository {} has duplicate pane id",
            status_render::display_text(&repo.id)
        ));
    }
    Ok(())
}

#[cfg(test)]
#[path = "status_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "status_exit_tests.rs"]
mod exit_tests;
