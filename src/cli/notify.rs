//! `nightcrow notify`: a program inside a terminal pane asks for its person.
//!
//! The pane is named by the token nightcrow put in that pane's environment, so
//! this only works from inside one — an agent's completion hook, or a long
//! command chained with `; nightcrow notify`.

use crate::backend::identity::PANE_TOKEN_ENV;
use crate::daemon::one_shot::request;
use crate::daemon::protocol::{ClientMessage, ServerMessage};
use anyhow::{Context, Result, bail};
use std::path::PathBuf;
use std::time::Duration;

/// Long enough for every open repository's hub to be asked in turn.
const NOTIFY_TIMEOUT: Duration = Duration::from_secs(10);

pub(crate) fn run_notify(message: Vec<String>, socket: Option<PathBuf>) -> Result<()> {
    let token = std::env::var(PANE_TOKEN_ENV).ok().filter(|t| !t.is_empty());
    let Some(token) = token else {
        bail!("{PANE_TOKEN_ENV} is not set: run `nightcrow notify` inside a nightcrow terminal");
    };
    let path = match socket {
        Some(path) => path,
        None => crate::daemon::socket::default_socket_path()
            .context("resolving the default daemon socket path")?,
    };
    let message = Some(message.join(" ")).filter(|m| !m.trim().is_empty());
    match request(
        &path,
        &ClientMessage::Notify { token, message },
        NOTIFY_TIMEOUT,
    )? {
        ServerMessage::Notified => Ok(()),
        ServerMessage::Error { message } => bail!("daemon refused the notice: {message}"),
        other => bail!("protocol error: unexpected answer to notify: {other:?}"),
    }
}
