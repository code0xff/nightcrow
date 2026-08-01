//! Putting the daemon into the background. Re-exec rather than fork: by the
//! time this is decided the process has not started its worker threads yet, but
//! the pattern is the trap either way — `fork` in a threaded process gives the
//! child one thread and every lock in whatever state it was in. Spawning a fresh
//! copy of this binary has no such state to inherit.
//!
//! The child gets its own session (`setsid`), so closing the terminal that
//! started it does not send it SIGHUP along with the shell's other children.
//! That is the whole difference from `&`.

use anyhow::{Context, Result};
use std::process::{Command, Stdio};

/// Environment marker telling a re-exec'd child it is already the background
/// copy, so it runs the daemon instead of spawning another one.
const ALREADY_DETACHED: &str = "NIGHTCROW_DETACHED";

/// Whether this process is the backgrounded copy.
pub fn is_detached_child() -> bool {
    marker_says_detached(std::env::var_os(ALREADY_DETACHED).as_deref())
}

/// The rule the marker carries, split from reading it.
///
/// Reading the environment inside the rule made the test answer for the
/// machine it ran on: a suite started from inside a nightcrow pane inherits
/// the marker from the daemon that spawned the pane, and the foreground case
/// then failed while saying nothing about the rule. Presence is what counts —
/// the child is spawned with `"1"`, but an empty value is still a marker.
fn marker_says_detached(marker: Option<&std::ffi::OsStr>) -> bool {
    marker.is_some()
}

/// Re-exec this binary in its own session and return.
///
/// The caller is the foreground copy and should exit: the returned pid is the
/// daemon. Output goes to `log_path`, appended, because a background process
/// with no terminal has nowhere else to put the address it is serving on or
/// the reason it failed to start.
pub fn respawn_in_background(log_path: &std::path::Path) -> Result<u32> {
    let exe = std::env::current_exe().context("locating the nightcrow binary")?;
    let args = child_args(std::env::args_os().skip(1));
    let mut command = background_command(&exe, &args, log_path)?;
    let child = command.spawn().context("starting the background daemon")?;
    Ok(child.id())
}

/// The arguments to hand the background copy: everything this one got, minus
/// the flag that sent it there — otherwise it would detach again, and again —
/// and minus `attach`. The background copy is always the daemon; `-d attach`
/// starts it from here and then attaches in *this* process.
fn child_args(args: impl Iterator<Item = std::ffi::OsString>) -> Vec<std::ffi::OsString> {
    args.filter(|arg| arg != "-d" && arg != "--detach" && arg != "attach")
        .collect()
}

/// Build the command that runs `exe` as a background session.
///
/// Separate from spawning it so the parts that can fail — the log directory,
/// the log file, the argument list — are testable without starting a process.
/// Nothing here execs anything.
fn background_command(
    exe: &std::path::Path,
    args: &[std::ffi::OsString],
    log_path: &std::path::Path,
) -> Result<Command> {
    if let Some(parent) = log_path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("creating {}", parent.display()))?;
    }
    // Appended and opened twice: the child owns both handles, and a single one
    // shared between stdout and stderr would have them overwrite each other's
    // offset.
    let out = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(log_path)
        .with_context(|| format!("opening {}", log_path.display()))?;
    let err = out
        .try_clone()
        .context("duplicating the daemon log handle")?;

    let mut command = Command::new(exe);
    command
        .args(args)
        .env(ALREADY_DETACHED, "1")
        // Not inherited: a background daemon reading the terminal would compete
        // with the shell for input, and be stopped by SIGTTIN if it tried.
        .stdin(Stdio::null())
        .stdout(Stdio::from(out))
        .stderr(Stdio::from(err));
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        // SAFETY: `pre_exec` runs between fork and exec, where only
        // async-signal-safe calls are allowed. `setsid` is one, and it is the
        // only call made here.
        unsafe {
            command.pre_exec(|| {
                if libc::setsid() == -1 {
                    return Err(std::io::Error::last_os_error());
                }
                Ok(())
            });
        }
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // setsid 의 Windows 대응. DETACHED_PROCESS 는 콘솔을 물려주지 않아
        // 이 터미널이 닫혀도 daemon 이 함께 죽지 않고, NEW_PROCESS_GROUP 은
        // 이 셸에 간 Ctrl-C 가 daemon 까지 가지 않게 한다.
        //
        // 대가: 콘솔이 없으므로 SetConsoleCtrlHandler 로 받을 이벤트가
        // 애초에 도착하지 않는다. 백그라운드 daemon 을 멈추는 경로는
        // 시그널이 아니라 `nightcrow stop` 이다 (PR 8).
        const DETACHED_PROCESS: u32 = 0x0000_0008;
        const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
        command.creation_flags(DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP);
    }
    Ok(command)
}

#[cfg(test)]
#[path = "detach_tests.rs"]
mod tests;
