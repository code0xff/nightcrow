//! Running a statusline command that is not ours, on a budget.
//!
//! Split out of `helper_statusline.rs` so that file decides *which* line gets
//! printed and this one is the process plumbing under it. Written for a caller
//! that must not be made to wait or to fail: the child is bounded, killed when
//! it overruns, reaped on every path, and any disappointment comes back as `None`.

use std::io::{Read, Write};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

/// A POSIX shell, resolved on `PATH`. Not `$SHELL`: an interactive shell would
/// read the user's rc files on every single refresh.
///
/// Windows included, and deliberately so: Claude Code runs a `statusLine`
/// through a POSIX shell on every platform, so the line being run here is one
/// `cmd.exe` would not run at all — the user would silently lose their
/// statusline. Which shell the *host's panes* use is a separate setting.
const SHELL: &str = "sh";
const SHELL_COMMAND_ARG: &str = "-c";

/// Used only when `sh` cannot be spawned at all, which on Windows means no Git
/// Bash on `PATH`. A command written for `cmd.exe` is the only kind that can
/// work there, so it is worth one attempt before giving the caller nothing.
#[cfg(windows)]
const FALLBACK_SHELL: &str = "cmd.exe";
#[cfg(windows)]
const FALLBACK_SHELL_COMMAND_ARG: &str = "/C";

/// Most stdout to take from a displaced command. A statusline is one short line;
/// this only stops a runaway script from growing this process.
const MAX_DELEGATED_STDOUT_BYTES: u64 = 64 * 1024;

/// How often to look for a child that has closed its stdout but not yet exited.
const EXIT_POLL: Duration = Duration::from_millis(2);

/// Run `command` with `raw` on its stdin and bring back what it printed, or
/// `None` if it could not be started, did not end well, or overran `budget`.
///
/// Through the platform shell's command mode (`sh -c` or `cmd.exe /C`), not an
/// argv we split ourselves: the provider documents that a `statusLine` command
/// "runs in a shell", and its examples (`~` paths, `jq` pipelines, inline
/// `$(...)`) rely on it. Re-splitting the string would quietly change what it
/// means — it is the user's own configuration, not ours to reinterpret.
pub(super) fn capture(command: &str, raw: &[u8], budget: Duration) -> Option<String> {
    let deadline = Instant::now() + budget;
    let mut child = spawn_shell(command)?;

    // Each pipe gets its own thread. Writing the payload first would deadlock
    // against a command that answers before draining its input, and reading first
    // would wedge a command that is still waiting for the rest of its input.
    if let Some(mut stdin) = child.stdin.take() {
        let payload = raw.to_vec();
        std::thread::spawn(move || {
            // A command that ignores its input closes the pipe early. That is a
            // choice it is allowed to make, not a failure of ours.
            let _ = stdin.write_all(&payload);
        });
    }
    let Some(stdout) = child.stdout.take() else {
        return abandon(child);
    };
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let mut captured = Vec::new();
        let _ = stdout
            .take(MAX_DELEGATED_STDOUT_BYTES)
            .read_to_end(&mut captured);
        let _ = tx.send(captured);
    });

    let Ok(captured) = rx.recv_timeout(remaining(deadline)) else {
        return abandon(child);
    };
    if !exited_well(&mut child, deadline) {
        return abandon(child);
    }
    // Output we cannot decode is output we cannot print; the caller has a line of
    // its own for that.
    let text = String::from_utf8(captured).ok()?;
    // The child owns the content of the line; we own its framing, and the caller
    // is the one that ends it with a newline. A command that printed nothing but
    // whitespace did not render a statusline at all.
    let printed = text.trim_end_matches(['\r', '\n']);
    if printed.trim().is_empty() {
        return None;
    }
    Some(printed.to_string())
}

/// Start `command` under a shell, with the pipes the caller needs.
///
/// stderr is discarded: Claude Code reads our stdout for the statusline, and a
/// chatty script's stderr shares the terminal with it, so a warning meant for a
/// log must not end up rendered as the line.
fn spawn_shell(command: &str) -> Option<Child> {
    match shell_child(SHELL, SHELL_COMMAND_ARG, command) {
        Ok(child) => Some(child),
        // Only a missing shell is worth a second try. A command that starts and
        // then fails is a command that ran, and running it again under a shell
        // it was not written for would just fail differently.
        #[cfg(windows)]
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            shell_child(FALLBACK_SHELL, FALLBACK_SHELL_COMMAND_ARG, command).ok()
        }
        Err(_) => None,
    }
}

fn shell_child(shell: &str, arg: &str, command: &str) -> std::io::Result<Child> {
    Command::new(shell)
        .arg(arg)
        .arg(command)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
}

/// Whether the child finished, and finished happily, before `deadline`.
///
/// Polled rather than waited on: `wait` has no timeout, and a command that
/// closes its stdout and then sleeps must not get to hold a refresh open.
fn exited_well(child: &mut Child, deadline: Instant) -> bool {
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return status.success(),
            Ok(None) if Instant::now() < deadline => std::thread::sleep(EXIT_POLL),
            _ => return false,
        }
    }
}

/// A child we are done waiting for. Killed so a wedged statusline command does not
/// outlive the refresh that started it, and reaped so it does not sit as a zombie
/// for whatever is left of this process's life.
fn abandon(mut child: Child) -> Option<String> {
    let _ = child.kill();
    let _ = child.wait();
    None
}

fn remaining(deadline: Instant) -> Duration {
    deadline.saturating_duration_since(Instant::now())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capture_executes_the_displaced_command_through_a_shell() {
        assert_eq!(
            capture("echo delegated", b"{}", Duration::from_secs(5)).as_deref(),
            Some("delegated")
        );
    }

    /// The case that cost a user their statusline: a real one is POSIX shell,
    /// and `cmd.exe` cannot run it on any platform.
    #[test]
    fn capture_runs_posix_shell_syntax() {
        assert_eq!(
            capture(
                "value=$(echo hud); export COLUMNS=${COLUMNS:-80}; echo \"${value}\"",
                b"{}",
                Duration::from_secs(5)
            )
            .as_deref(),
            Some("hud")
        );
    }

    #[test]
    fn capture_hands_the_payload_to_the_command_on_stdin() {
        assert_eq!(
            capture("cat", b"from-claude", Duration::from_secs(5)).as_deref(),
            Some("from-claude")
        );
    }
}
