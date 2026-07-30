//! Running a statusline command that is not ours, on a budget.
//!
//! Split out of `helper_statusline.rs` so that file decides *which* line gets
//! printed and this one is the process plumbing under it. Everything here is
//! written for a caller that must not be made to wait and must not be made to
//! fail: the child is bounded, killed when it overruns, reaped on every path, and
//! any disappointment comes back as `None`.

use std::io::{Read, Write};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

/// A POSIX shell, resolved on `PATH`. Not `$SHELL`: an interactive shell would
/// read the user's rc files on every single refresh.
const SHELL: &str = "sh";

/// Most stdout to take from a displaced command. A statusline is one short line;
/// this only stops a runaway script from growing this process.
const MAX_DELEGATED_STDOUT_BYTES: u64 = 64 * 1024;

/// How often to look for a child that has closed its stdout but not yet exited.
const EXIT_POLL: Duration = Duration::from_millis(2);

/// Run `command` with `raw` on its stdin and bring back what it printed, or
/// `None` if it could not be started, did not end well, or overran `budget`.
///
/// Through `sh -c`, not an argv we split ourselves: the provider documents that a
/// `statusLine` command "runs in a shell", and its own examples rely on it — a `~`
/// path, a `jq` pipeline, an inline `$(...)`. Re-splitting the string the user wrote
/// would quietly change what it means. This is the user's own configuration rather
/// than input from a stranger, but it is also not ours to reinterpret.
pub(super) fn capture(command: &str, raw: &[u8], budget: Duration) -> Option<String> {
    let deadline = Instant::now() + budget;
    let mut child = Command::new(SHELL)
        .arg("-c")
        .arg(command)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        // Claude Code reads our stdout for the statusline, and a chatty script's
        // stderr shares the terminal with it. Discarded, so a warning meant for a
        // log cannot end up rendered as the statusline.
        .stderr(Stdio::null())
        .spawn()
        .ok()?;

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

/// Whether the child finished, and finished happily, before `deadline`.
///
/// Polled rather than waited on: `wait` has no timeout, and a command that closes
/// its stdout and then sleeps must not get to hold a refresh open. Stdout is
/// already at EOF by the time this is called, so the first look nearly always
/// finds the child gone.
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
