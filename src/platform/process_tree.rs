//! The lifetime boundary for a terminal pane's child processes.
//!
//! `portable-pty` creates a new Unix session for every pane. The shell may
//! then put jobs in separate process groups, so killing only the shell's group
//! leaves jobs behind. Linux and macOS enumerate the session before teardown
//! and kill every group in it; other Unix targets still get the root and
//! foreground groups, followed by closing the PTY master in `PtyPane::drop`. Windows uses
//! a Job Object, whose kernel membership includes descendants created after
//! the child is assigned.

use anyhow::{Context as _, Result};
use portable_pty::{Child, MasterPty};
use std::io;

#[cfg(unix)]
#[derive(Debug)]
pub(crate) struct ProcessTree {
    process_group: libc::pid_t,
    session: libc::pid_t,
    tty: Option<libc::c_int>,
}

#[cfg(windows)]
#[derive(Debug)]
pub(crate) struct ProcessTree {
    job: std::os::windows::io::OwnedHandle,
}

impl ProcessTree {
    /// Establish the platform boundary immediately after the PTY child is
    /// created. A failure is returned so an unowned pane is never admitted.
    pub(crate) fn attach(child: &dyn Child, master: &dyn MasterPty) -> Result<Self> {
        #[cfg(unix)]
        {
            let pid = child
                .process_id()
                .ok_or_else(|| anyhow::anyhow!("PTY child did not expose a process id"))?
                as libc::pid_t;
            if pid <= 1 {
                anyhow::bail!("refusing unsafe PTY process id {pid}");
            }

            // portable-pty calls setsid() before exec, which makes the child a
            // session and process-group leader. Check the session while the
            // child is alive so a recycled PID can never be mistaken for ours.
            let process_group = unsafe { libc::getpgid(pid) };
            let session = unsafe { libc::getsid(pid) };
            if process_group <= 1 || session <= 1 {
                return Err(io::Error::last_os_error()).context("querying PTY process session");
            }
            Ok(Self {
                process_group,
                session,
                tty: master.as_raw_fd(),
            })
        }

        #[cfg(windows)]
        {
            use std::os::windows::io::{AsRawHandle, FromRawHandle};
            use windows_sys::Win32::Foundation::HANDLE;
            use windows_sys::Win32::System::JobObjects::{
                AssignProcessToJobObject, CreateJobObjectW, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
                JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JobObjectExtendedLimitInformation,
                SetInformationJobObject,
            };

            let process = child
                .as_raw_handle()
                .ok_or_else(|| anyhow::anyhow!("PTY child did not expose a process handle"))?;
            let _ = master;
            let raw_job = unsafe { CreateJobObjectW(std::ptr::null(), std::ptr::null()) };
            if raw_job.is_null() {
                return Err(io::Error::last_os_error()).context("creating pane Job Object");
            }

            // OwnedHandle closes the job on every return path. The limit flag
            // makes that close a final tree termination if setup is interrupted
            // before an explicit TerminateJobObject call.
            let job = unsafe { std::os::windows::io::OwnedHandle::from_raw_handle(raw_job) };
            let mut limits = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
            limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
            let job_handle = job.as_raw_handle() as HANDLE;
            let set_ok = unsafe {
                SetInformationJobObject(
                    job_handle,
                    JobObjectExtendedLimitInformation,
                    (&limits as *const JOBOBJECT_EXTENDED_LIMIT_INFORMATION).cast(),
                    std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
                )
            };
            if set_ok == 0 {
                return Err(io::Error::last_os_error()).context("configuring pane Job Object");
            }
            // Assignment is necessarily after portable-pty's CreateProcessW;
            // the direct child kill in `PtyPane::drop` covers a setup failure,
            // while the job owns descendants created after this point.
            let assign_ok = unsafe { AssignProcessToJobObject(job_handle, process as HANDLE) };
            if assign_ok == 0 {
                return Err(io::Error::last_os_error())
                    .context("assigning PTY child to pane Job Object");
            }
            Ok(Self { job })
        }
    }

    /// Terminate the pane's process tree. An already exited tree is normal
    /// during backend teardown and is treated as success.
    pub(crate) fn terminate(&self) -> io::Result<()> {
        #[cfg(unix)]
        {
            let mut first_error = None;
            for pass in 0..3 {
                let mut groups = std::collections::BTreeSet::new();
                if pass == 0 {
                    groups.insert(self.process_group);
                    if let Some(foreground) = self.foreground_group() {
                        groups.insert(foreground);
                    }
                }
                #[cfg(any(target_os = "linux", target_os = "android", target_os = "macos"))]
                groups.extend(session_process_groups(self.session));

                for &group in &groups {
                    if let Err(error) = kill_group(group) {
                        first_error.get_or_insert(error);
                    }
                }
            }
            first_error.map_or(Ok(()), Err)
        }

        #[cfg(windows)]
        {
            use std::os::windows::io::AsRawHandle;
            use windows_sys::Win32::Foundation::ERROR_INVALID_HANDLE;
            use windows_sys::Win32::System::JobObjects::TerminateJobObject;

            let result = unsafe { TerminateJobObject(self.job.as_raw_handle() as _, 1) };
            if result != 0 {
                return Ok(());
            }
            let error = io::Error::last_os_error();
            if error.raw_os_error() == Some(ERROR_INVALID_HANDLE as i32) {
                Ok(())
            } else {
                Err(error)
            }
        }
    }

    #[cfg(unix)]
    fn foreground_group(&self) -> Option<libc::pid_t> {
        let tty = self.tty?;
        let group = unsafe { libc::tcgetpgrp(tty) };
        if group <= 1 {
            return None;
        }
        // `tcgetpgrp` identifies the current controlling terminal's group. A
        // session check prevents a stale/reused group id from escaping the
        // pane boundary.
        let session = unsafe { libc::getsid(group) };
        (session == self.session).then_some(group)
    }
}

#[cfg(unix)]
fn kill_group(group: libc::pid_t) -> io::Result<()> {
    if group <= 1 {
        return Ok(());
    }
    let result = unsafe { libc::kill(-group, libc::SIGKILL) };
    if result == 0 {
        Ok(())
    } else {
        let error = io::Error::last_os_error();
        if error.raw_os_error() == Some(libc::ESRCH) {
            Ok(())
        } else {
            Err(error)
        }
    }
}

#[cfg(any(target_os = "linux", target_os = "android"))]
fn session_process_groups(session: libc::pid_t) -> impl Iterator<Item = libc::pid_t> {
    let mut groups = std::collections::BTreeSet::new();
    let Ok(entries) = std::fs::read_dir("/proc") else {
        return groups.into_iter();
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let Some(name) = name.to_str() else { continue };
        if !name.bytes().all(|byte| byte.is_ascii_digit()) {
            continue;
        }
        let Ok(stat) = std::fs::read_to_string(entry.path().join("stat")) else {
            continue;
        };
        let Some((_, group, proc_session)) = parse_proc_stat(&stat) else {
            continue;
        };
        if proc_session == session && group > 1 {
            groups.insert(group);
        }
    }
    groups.into_iter()
}

#[cfg(target_os = "macos")]
fn session_process_groups(session: libc::pid_t) -> impl Iterator<Item = libc::pid_t> {
    let mut groups = std::collections::BTreeSet::new();
    let Ok(output) = std::process::Command::new("/bin/ps")
        .args(["-axo", "pgid=,sid="])
        .output()
    else {
        return groups.into_iter();
    };
    if !output.status.success() {
        return groups.into_iter();
    }
    for line in String::from_utf8_lossy(&output.stdout).lines() {
        let mut fields = line.split_whitespace();
        let Some(group) = fields.next().and_then(|value| value.parse().ok()) else {
            continue;
        };
        let Some(proc_session) = fields.next().and_then(|value| value.parse().ok()) else {
            continue;
        };
        if proc_session == session && group > 1 {
            groups.insert(group);
        }
    }
    groups.into_iter()
}

#[cfg(any(target_os = "linux", target_os = "android"))]
fn parse_proc_stat(stat: &str) -> Option<(libc::pid_t, libc::pid_t, libc::pid_t)> {
    let close = stat.rfind(')')?;
    let pid = stat[..stat.find(' ')?].parse().ok()?;
    let mut fields = stat.get(close + 2..)?.split_whitespace();
    fields.next()?; // state
    let _parent = fields.next()?.parse::<libc::pid_t>().ok()?;
    let group = fields.next()?.parse::<libc::pid_t>().ok()?;
    let session = fields.next()?.parse::<libc::pid_t>().ok()?;
    Some((pid, group, session))
}

#[cfg(all(test, any(target_os = "linux", target_os = "android")))]
mod tests {
    use super::parse_proc_stat;

    #[test]
    fn proc_stat_parser_handles_spaces_and_parentheses_in_command_names() {
        let stat = "123 (worker with ) mark) S 1 122 122 0 0 0";
        assert_eq!(parse_proc_stat(stat), Some((123, 122, 122)));
    }
}
