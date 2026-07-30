use super::{PtyBackend, PtyEvent, PtyPane};
use crate::backend::PaneId;
use crate::backend::identity::{PANE_TOKEN_ENV, PaneIdentity};
use crate::backend::slot::{PaneLaunch, resume_command_line};
use anyhow::Result;
use portable_pty::{CommandBuilder, NativePtySystem, PtySize, PtySystem};
use std::io::Read;
use std::sync::mpsc;
use std::thread;
use std::time::Instant;

impl PtyBackend {
    /// Open a pane and say which one it is.
    ///
    /// The trait reports panes as events, because a backend serving a shared
    /// session cannot answer on the spot. This one can, and the terminal hub —
    /// which owns a `PtyBackend` outright rather than through the trait — needs
    /// the id to register the pane before anything else happens to it.
    pub fn open_pane(&mut self, rows: u16, cols: u16, command: Option<&str>) -> Result<PaneId> {
        let identity = PaneIdentity::new()?;
        let launch = PaneLaunch {
            command: command.map(str::to_string),
        };
        self.spawn_pane(rows, cols, command, identity, launch)
    }

    /// Replace an exited pane's process, keeping the slot it ran in.
    ///
    /// A new `PaneId` is unavoidable: ids are monotonic and every client treats
    /// `Exited` as final for one. The slot's token is what carries over, so an
    /// observer that has been tracking this pane keeps its place, and the
    /// generation moves so decisions made about the old process cannot land on
    /// the new one.
    ///
    /// The composed command line is checked before anything is torn down, so a
    /// refused relaunch leaves the pane exactly as it was.
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
        // replacement's is started.
        self.panes.remove(&id);
        self.slots.remove(id);

        // The retained launch stays the *original* invocation. Carrying the
        // composed line forward instead would accumulate resume arguments on
        // every further relaunch.
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
        // Reserve the next id only after every fallible PTY/spawn step succeeds,
        // so a failure here does not consume an id slot.
        let pty_system = NativePtySystem::default();
        let pair = pty_system.openpty(PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        })?;

        let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".to_string());
        let mut cmd = CommandBuilder::new(&shell);
        // A reserved startup command runs through the login shell's `-lc`:
        // the command text is passed as a single argv item, so the shell —
        // not us — handles its quoting/word-splitting. This avoids the race
        // of spawning a shell and later injecting `command\r`, and avoids any
        // string interpolation into a wrapper on our side.
        if let Some(command) = command {
            cmd.arg("-lc");
            cmd.arg(command);
        }
        cmd.env("TERM", "xterm-256color");
        // Set at spawn time because a child cannot be told afterwards, and the
        // provider's own helper processes inherit it — that inheritance is what
        // lets an out-of-process observer name the pane an event came from.
        cmd.env(PANE_TOKEN_ENV, identity.token.as_str());
        // Only set cwd if the directory actually exists; otherwise inherit
        // ours so spawn does not fail outright (matters for unit tests that
        // pass placeholder paths).
        if let Ok(canonical) = self.cwd.canonicalize() {
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

        let wait_handle = thread::spawn(move || {
            let _ = child.wait();
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
            },
        );
        self.slots.insert(id, identity, launch, Instant::now());
        Ok(id)
    }
}
