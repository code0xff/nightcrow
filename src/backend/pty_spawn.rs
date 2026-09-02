use super::{ExitPhase, PtyBackend, PtyEvent, PtyPane};
use crate::backend::PaneId;
use crate::backend::identity::{PANE_TOKEN_ENV, PLUGIN_RUNTIME_DIR_ENV, PaneIdentity};
use crate::backend::slot::{PaneLaunch, resume_command_line};
use anyhow::Result;
use portable_pty::{CommandBuilder, NativePtySystem, PtySize, PtySystem};
use std::io::Read;
#[cfg(windows)]
use std::path::{Component, Path, Prefix};
use std::sync::mpsc;
use std::thread;
use std::time::Instant;

impl PtyBackend {
    /// Open a pane and say which one it is. Returns the id directly — unlike
    /// the trait — because the terminal hub owns a `PtyBackend` outright and
    /// needs the id to register the pane before anything else happens to it.
    pub fn open_pane(&mut self, rows: u16, cols: u16, command: Option<&str>) -> Result<PaneId> {
        let identity = PaneIdentity::new()?;
        let launch = PaneLaunch {
            command: command.map(str::to_string),
        };
        self.spawn_pane(rows, cols, command, identity, launch)
    }

    /// Replace an exited pane's process, keeping the slot it ran in. A new
    /// `PaneId` is unavoidable: ids are monotonic and every client treats
    /// `Exited` as final. The slot's token carries over, so an observer keeps
    /// its place; the generation moves so decisions about the old process
    /// cannot land on the new one.
    pub fn relaunch_pane(
        &mut self,
        id: PaneId,
        rows: u16,
        cols: u16,
        resume_args: &[String],
        allowed_flags: &[String],
    ) -> Result<PaneId> {
        let slot = self
            .slots
            .get(id)
            .ok_or_else(|| anyhow::anyhow!("pane {id} has no slot to relaunch"))?;
        let launch = slot.launch.clone();
        let mut identity = slot.identity.clone();
        let line = resume_command_line(launch.command.as_deref(), resume_args, allowed_flags)?;

        identity.advance();
        // Retire the old process first: two children writing one slot's PTY
        // would interleave, and the reader thread has to be let go before the
        // replacement's is started. The composed line was checked above, so a
        // refused relaunch never tears anything down.
        self.panes.remove(&id);
        self.slots.remove(id);

        // The retained launch stays the *original* invocation; carrying the
        // composed line forward would accumulate resume arguments on every
        // further relaunch.
        self.spawn_pane(rows, cols, Some(line.as_str()), identity, launch)
    }

    fn spawn_pane(
        &mut self,
        rows: u16,
        cols: u16,
        command: Option<&str>,
        identity: PaneIdentity,
        launch: PaneLaunch,
    ) -> Result<PaneId> {
        // Reserve the next id only after every fallible PTY/spawn step
        // succeeds, so a failure here does not consume an id slot.
        let pty_system = NativePtySystem::default();
        let pair = pty_system.openpty(PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        })?;

        let shell = self.shell.resolved_program();
        let mut cmd = CommandBuilder::new(&shell);
        // A reserved startup command goes through the shell's configured args
        // as a single argv item, so the shell handles its quoting. This avoids
        // the race of spawning a shell and later injecting `command\r`.
        if let Some(command) = command {
            for arg in self.shell.command_args() {
                cmd.arg(arg);
            }
            cmd.arg(command);
        }
        crate::backend::pane_env::apply(&mut cmd);
        // Set at spawn time because a child cannot be told afterwards, and the
        // provider's own helper processes inherit it — that inheritance is what
        // lets an out-of-process observer name the pane an event came from.
        cmd.env(PANE_TOKEN_ENV, identity.token.as_str());
        // Alongside the token: a provider's hook is told which pane it is in
        // *and* where that pane's plugins listen. The plugin spawn derives
        // this from the same hub path, so the two agree without either being
        // told by the other.
        if let Some(dir) =
            crate::backend::identity::plugin_runtime_dir(std::path::Path::new(&self.cwd))
        {
            cmd.env(PLUGIN_RUNTIME_DIR_ENV, dir);
        }
        // Only set cwd if the directory actually exists; otherwise inherit
        // ours so spawn does not fail outright (matters for unit tests that
        // pass placeholder paths). The clean canonicalize strips the Windows
        // verbatim prefix (`\\?\`) that cmd.exe rejects as a UNC path.
        if let Ok(canonical) = crate::platform::paths::canonicalize_clean(&self.cwd) {
            #[cfg(windows)]
            ensure_windows_shell_supports_cwd(&shell, &canonical)?;
            cmd.cwd(canonical);
        }
        let mut child = pair.slave.spawn_command(cmd)?;
        let killer = child.clone_killer();
        drop(pair.slave);

        let mut reader = pair.master.try_clone_reader()?;
        let writer = pair.master.take_writer()?;

        let id = self.next_id;
        let next = id
            .checked_add(1)
            .ok_or_else(|| anyhow::anyhow!("pane id counter overflow"))?;
        self.next_id = next;

        let (tx, rx) = mpsc::channel();
        let exit_tx = tx.clone();
        let reader_handle = thread::spawn(move || {
            let mut buf = [0u8; 4096];
            loop {
                match reader.read(&mut buf) {
                    Ok(0) | Err(_) => break,
                    Ok(n) => {
                        if tx.send(PtyEvent::Output(buf[..n].to_vec())).is_err() {
                            break;
                        }
                    }
                }
            }
            let _ = tx.send(PtyEvent::Exited);
        });

        // The child's death is reported, not just awaited: on Windows the
        // pseudoconsole holds the pipe open until the master drops, so the
        // reader above never reaches EOF. `ExitPhase` handles the draining.
        let wait_handle = thread::spawn(move || {
            let _ = child.wait();
            let _ = exit_tx.send(PtyEvent::ChildExited);
        });

        self.panes.insert(
            id,
            PtyPane {
                master: Some(pair.master),
                writer: Some(writer),
                killer,
                rx,
                reader_handle: Some(reader_handle),
                wait_handle: Some(wait_handle),
                exit: ExitPhase::Running,
            },
        );
        self.slots.insert(id, identity, launch, Instant::now());
        Ok(id)
    }
}

#[cfg(windows)]
fn ensure_windows_shell_supports_cwd(shell: &str, cwd: &Path) -> Result<()> {
    let is_cmd = Path::new(shell)
        .file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| {
            name.eq_ignore_ascii_case("cmd") || name.eq_ignore_ascii_case("cmd.exe")
        });
    let is_unc = matches!(
        cwd.components().next(),
        Some(Component::Prefix(prefix))
            if matches!(prefix.kind(), Prefix::UNC(_, _) | Prefix::VerbatimUNC(_, _))
    );
    if is_cmd && is_unc {
        anyhow::bail!(
            "cmd.exe cannot use UNC working directory {}; configure [shell].program to PowerShell or another UNC-capable shell",
            cwd.display()
        );
    }
    Ok(())
}

#[cfg(all(test, windows))]
mod windows_cwd_tests {
    use super::ensure_windows_shell_supports_cwd;
    use std::path::Path;

    #[test]
    fn cmd_rejects_unc_but_not_drive_working_directories() {
        assert!(
            ensure_windows_shell_supports_cwd("cmd.exe", Path::new(r"\\server\share\repo"))
                .is_err()
        );
        assert!(ensure_windows_shell_supports_cwd("cmd.exe", Path::new(r"C:\repo")).is_ok());
        assert!(
            ensure_windows_shell_supports_cwd("pwsh.exe", Path::new(r"\\server\share\repo"))
                .is_ok()
        );
    }
}
