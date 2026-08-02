//! The console dispositions a process hands down to the shells it spawns.
//!
//! On Windows "ignore Ctrl-C" is a property of the process and children inherit
//! it. `daemon::detach` passes `CREATE_NEW_PROCESS_GROUP`, which sets that flag,
//! so every ConPTY pane inherits it too.
//!
//! The symptom is deceptive: conhost still delivers the `0x03` byte, so an idle
//! prompt abandons its line and looks wired up, but no `CTRL_C_EVENT` is raised
//! and a *running* program (`ping`, a Python script) is never interrupted.

use anyhow::Result;

#[cfg(unix)]
mod imp {
    use anyhow::Result;

    /// Nothing to clear: SIGINT generation is the tty's `ISIG` bit, which every
    /// PTY gets fresh from the kernel.
    pub(super) fn inherit_ctrl_c_as_an_event() -> Result<()> {
        Ok(())
    }
}

#[cfg(windows)]
mod imp {
    use anyhow::{Result, bail};
    use windows_sys::Win32::System::Console::SetConsoleCtrlHandler;

    pub(super) fn inherit_ctrl_c_as_an_event() -> Result<()> {
        // A null routine clears the ignore flag rather than unregistering
        // anyone; handlers `ctrlc` installed are separate entries and survive.
        //
        // SAFETY: a plain Win32 call with no memory contract to uphold.
        if unsafe { SetConsoleCtrlHandler(None, 0) } == 0 {
            bail!(
                "clearing the inherited ignore-Ctrl-C disposition: {}",
                std::io::Error::last_os_error()
            );
        }
        Ok(())
    }
}

/// Make Ctrl-C reach spawned children as an interrupt, not just as a byte.
///
/// Call once, before the first pane is spawned — the disposition is copied into
/// a child at creation, so clearing it afterwards does nothing for panes that
/// already exist.
pub(crate) fn inherit_ctrl_c_as_an_event() -> Result<()> {
    imp::inherit_ctrl_c_as_an_event()
}

#[cfg(test)]
#[path = "console_tests.rs"]
mod tests;
