use crate::daemon::client::{ConnectError, DaemonClient};
use crate::daemon::version_mismatch::VersionMismatch;
use anyhow::Result;
use std::io::{BufRead, IsTerminal, Write};
use std::path::Path;
use std::process::Child;

/// Start a session when needed, then attach to it.
pub(crate) fn run_attach_detached(yes: bool) -> Result<()> {
    let socket = crate::daemon::socket::default_socket_path()?;
    let log = super::daemon::daemon_output_path()?;
    let client = connect_or_start(&socket, &log, yes)?;
    crate::application::attach::run_attach(client)
}

fn connect_or_start(socket: &Path, log: &Path, yes: bool) -> Result<DaemonClient> {
    match DaemonClient::connect_for_attach(socket) {
        Ok(client) => Ok(client),
        Err(ConnectError::Unavailable(first_unavailable)) => {
            let mut child = start_background_daemon(log)?;
            super::startup::wait_for_daemon(&mut child, socket, log, first_unavailable)
        }
        Err(ConnectError::Failed(error)) => {
            let Some(mismatch) = error.downcast_ref::<VersionMismatch>().cloned() else {
                return Err(error);
            };
            restart_for_version_mismatch(socket, log, error, mismatch, yes)
        }
    }
}

fn restart_for_version_mismatch(
    socket: &Path,
    log: &Path,
    original_error: anyhow::Error,
    mismatch: VersionMismatch,
    yes: bool,
) -> Result<DaemonClient> {
    let context = RestartContext {
        socket,
        log,
        original_error,
        mismatch,
    };
    let operations = RestartOperations {
        stop: |socket: &Path| crate::cli::stop::run_stop(Some(socket.to_path_buf())),
        start: start_background_daemon,
        wait: super::startup::wait_for_daemon,
    };
    if yes {
        return restart_for_mismatch(context, RestartPrompt::preapproved(), operations);
    }

    let stdin = std::io::stdin();
    let stderr = std::io::stderr();
    if !stdin.is_terminal() || !stderr.is_terminal() {
        return restart_for_mismatch(context, RestartPrompt::unavailable(), operations);
    }
    let mut input = stdin.lock();
    let mut output = stderr.lock();
    restart_for_mismatch(
        context,
        RestartPrompt::interactive(&mut input, &mut output),
        operations,
    )
}

fn start_background_daemon(log: &Path) -> Result<Child> {
    let child = crate::daemon::detach::respawn_in_background(log)?;
    eprintln!(
        "nightcrow: started a session in the background (pid {})",
        child.id()
    );
    eprintln!("nightcrow: its output goes to {}", log.display());
    Ok(child)
}

struct RestartContext<'a> {
    socket: &'a Path,
    log: &'a Path,
    original_error: anyhow::Error,
    mismatch: VersionMismatch,
}

struct RestartPrompt<'a> {
    input: Option<&'a mut dyn BufRead>,
    output: Option<&'a mut dyn Write>,
    assume_yes: bool,
}

impl<'a> RestartPrompt<'a> {
    fn preapproved() -> Self {
        Self {
            input: None,
            output: None,
            assume_yes: true,
        }
    }

    fn unavailable() -> Self {
        Self {
            input: None,
            output: None,
            assume_yes: false,
        }
    }

    fn interactive(input: &'a mut dyn BufRead, output: &'a mut dyn Write) -> Self {
        Self {
            input: Some(input),
            output: Some(output),
            assume_yes: false,
        }
    }
}

struct RestartOperations<Stop, Start, Wait> {
    stop: Stop,
    start: Start,
    wait: Wait,
}

fn restart_for_mismatch<C, R, Stop, Start, Wait>(
    context: RestartContext<'_>,
    prompt: RestartPrompt<'_>,
    operations: RestartOperations<Stop, Start, Wait>,
) -> Result<R>
where
    Stop: FnOnce(&Path) -> Result<()>,
    Start: FnOnce(&Path) -> Result<C>,
    Wait: FnOnce(&mut C, &Path, &Path, anyhow::Error) -> Result<R>,
{
    if !confirm_restart(&context.mismatch, prompt)? {
        return Err(manual_restart_error(context.original_error, context.socket));
    }

    (operations.stop)(context.socket)?;
    let mut child = (operations.start)(context.log)?;
    let waiting = anyhow::anyhow!(
        "waiting for the restarted daemon on {}",
        context.socket.display()
    );
    (operations.wait)(&mut child, context.socket, context.log, waiting)
}

fn confirm_restart(mismatch: &VersionMismatch, mut prompt: RestartPrompt<'_>) -> Result<bool> {
    if prompt.assume_yes {
        return Ok(true);
    }
    let (Some(input), Some(output)) = (&mut prompt.input, &mut prompt.output) else {
        return Ok(false);
    };

    writeln!(output, "nightcrow: version mismatch: {mismatch}.")?;
    writeln!(
        output,
        "Restarting will stop the daemon and terminate every terminal pane and running program."
    )?;
    write!(output, "Restart the session now? [y/N] ")?;
    output.flush()?;

    let mut response = String::new();
    input.read_line(&mut response)?;
    Ok(matches!(
        response.trim().to_ascii_lowercase().as_str(),
        "y" | "yes"
    ))
}

fn manual_restart_error(error: anyhow::Error, socket: &Path) -> anyhow::Error {
    error.context(format!(
        "restart the session manually with `nightcrow stop --socket {}` followed by `nightcrow attach`",
        socket.display()
    ))
}

#[cfg(test)]
#[path = "attach_tests.rs"]
mod tests;
#[cfg(test)]
#[path = "attach_yes_tests.rs"]
mod yes_tests;
