//! One long-lived child process per enabled plugin.
//!
//! The host owns the process and three pump threads and exposes two non-blocking
//! operations: queue an event, take a decoded command. Nothing a plugin does can
//! make either block — the terminal hub calls them on the thread that also
//! serves every pane.

use super::host_pump;
use super::protocol::{PROTOCOL_VERSION, PluginCommand, PluginEvent, encode_event};
use crate::config::PluginConfig;
use crate::platform::threading::{REAP_TIMEOUT, try_timed_join};
use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, SyncSender, TrySendError};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

/// Events that may sit unwritten before further ones are dropped.
///
/// Bounded on purpose: the queue exists to absorb a plugin that is briefly busy,
/// not to buffer one that has stopped reading. Deep enough to cover a burst of
/// output from every pane at once, shallow enough that a wedged plugin's backlog
/// is bounded memory rather than a growing one.
pub const OUTBOUND_QUEUE_DEPTH: usize = 256;

/// Commands that may sit undrained before the reader stops accepting more.
///
/// Bounded for the same reason the outbound queue is, from the other direction:
/// the hub drains only a handful per tick, so an unbounded inbound queue let a
/// plugin writing faster than that grow the host's memory without limit.
///
/// Unlike the outbound side this blocks rather than drops — the reader thread
/// stalls, the plugin's stdout pipe fills, and the plugin blocks writing. That
/// is backpressure onto whoever is being too loud, and it loses no command a
/// well-behaved plugin sent. Shutdown still ends the thread, because dropping
/// the receiver makes the blocked send fail.
const INBOUND_QUEUE_DEPTH: usize = 256;

/// How long a plugin gets to exit on its own after being told to.
const CHILD_EXIT_GRACE: Duration = Duration::from_millis(200);

/// Gap between `try_wait` polls while waiting out [`CHILD_EXIT_GRACE`].
const CHILD_EXIT_POLL: Duration = Duration::from_millis(5);

pub struct PluginHost {
    name: String,
    /// Behind a mutex so [`Self::is_alive`] can ask the OS without `&mut`: the
    /// hub holds hosts immutably while dispatching events.
    child: Mutex<Child>,
    /// Held in an `Option` so shutdown can drop it, which is what ends the
    /// writer thread and closes the plugin's stdin.
    events: Option<SyncSender<String>>,
    commands: Receiver<PluginCommand>,
    dropped: Arc<AtomicU64>,
    writer: Option<JoinHandle<()>>,
    reader: Option<JoinHandle<()>>,
    stderr: Option<JoinHandle<()>>,
    shut_down: bool,
}

/// Keep a plugin from opening a console window of its own.
///
/// A backgrounded session runs `DETACHED_PROCESS`, so it has no console to hand
/// down. Windows answers that by allocating a *new* console for a
/// console-subsystem child — one visible window per plugin, and a window the
/// user can close, which kills the plugin under it. Every pipe this child uses
/// is one the spawn opened, so it has nothing to show a console for.
///
/// Unix inherits no console this way and needs no flag.
fn no_console_window(command: &mut Command) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    #[cfg(not(windows))]
    {
        let _ = command;
    }
}

impl PluginHost {
    /// Launch `cfg.command` and start pumping.
    ///
    /// Resolution order for the program: a `cfg.command` containing a path
    /// separator is taken as a path and used as given; otherwise `plugin_dir` is
    /// searched first, so an installed plugin wins over a same-named binary on
    /// the user's `PATH`, and only if it is not there is the bare name handed to
    /// the OS to resolve against `PATH`.
    ///
    /// No pane token is passed in the environment. A plugin learns which panes
    /// exist only from the events it is sent, which is what keeps a plugin from
    /// addressing a pane that never opted in to it.
    pub fn spawn(
        cfg: &PluginConfig,
        plugin_dir: Option<&Path>,
        runtime_dir: Option<&Path>,
    ) -> Result<PluginHost> {
        Self::spawn_with_queue_depth(cfg, plugin_dir, runtime_dir, OUTBOUND_QUEUE_DEPTH)
    }

    fn spawn_with_queue_depth(
        cfg: &PluginConfig,
        plugin_dir: Option<&Path>,
        runtime_dir: Option<&Path>,
        depth: usize,
    ) -> Result<PluginHost> {
        let program = resolve_program(&cfg.command, plugin_dir);
        let mut command = Command::new(&program);
        command
            .args(&cfg.args)
            .envs(&cfg.env)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        // After `cfg.env`, so this is not something a config can point at
        // another hub's socket: which hub a plugin belongs to is the host's to
        // say. See `PLUGIN_RUNTIME_DIR_ENV`.
        if let Some(dir) = runtime_dir {
            command.env(crate::backend::identity::PLUGIN_RUNTIME_DIR_ENV, dir);
        }
        no_console_window(&mut command);
        let mut child = command.spawn().with_context(|| {
            format!(
                "cannot launch plugin \"{}\" from {}",
                cfg.name,
                program.display()
            )
        })?;

        // Every pipe was requested above, so these are present; the context
        // still says which one is missing rather than unwrapping blind.
        let stdin = child.stdin.take().context("plugin stdin pipe missing")?;
        let stdout = child.stdout.take().context("plugin stdout pipe missing")?;
        let stderr = child.stderr.take().context("plugin stderr pipe missing")?;

        let (events_tx, events_rx) = mpsc::sync_channel::<String>(depth);
        let (commands_tx, commands_rx) = mpsc::sync_channel::<PluginCommand>(INBOUND_QUEUE_DEPTH);
        let name = cfg.name.clone();

        let writer_name = name.clone();
        let reader_name = name.clone();
        let stderr_name = name.clone();
        Ok(PluginHost {
            name,
            child: Mutex::new(child),
            events: Some(events_tx),
            commands: commands_rx,
            dropped: Arc::new(AtomicU64::new(0)),
            writer: Some(thread::spawn(move || {
                host_pump::write_events(stdin, events_rx, writer_name)
            })),
            reader: Some(thread::spawn(move || {
                host_pump::read_commands(stdout, commands_tx, reader_name)
            })),
            stderr: Some(thread::spawn(move || {
                host_pump::drain_stderr(stderr, stderr_name)
            })),
            shut_down: false,
        })
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    /// Queue one event. `false` means it was not queued — the plugin is behind,
    /// gone, or the event could not be encoded — and the caller carries on
    /// regardless: a slow plugin must never stall the pane it is watching.
    pub fn send(&self, ev: &PluginEvent) -> bool {
        let line = match encode_event(ev) {
            Ok(line) => line,
            Err(error) => {
                tracing::warn!(plugin = %self.name, %error, "dropping unencodable plugin event");
                self.dropped.fetch_add(1, Ordering::Relaxed);
                return false;
            }
        };
        let Some(events) = self.events.as_ref() else {
            return false;
        };
        match events.try_send(line) {
            Ok(()) => true,
            Err(TrySendError::Full(_)) => {
                self.dropped.fetch_add(1, Ordering::Relaxed);
                false
            }
            Err(TrySendError::Disconnected(_)) => false,
        }
    }

    /// Take one decoded command, if the plugin has sent one. Never blocks.
    pub fn try_recv(&self) -> Option<PluginCommand> {
        self.commands.try_recv().ok()
    }

    /// How many events were thrown away rather than queued. Rises whenever the
    /// plugin cannot keep up, so it is the signal that a plugin is wedged.
    pub fn dropped_events(&self) -> u64 {
        self.dropped.load(Ordering::Relaxed)
    }

    pub fn is_alive(&self) -> bool {
        matches!(self.locked_child().try_wait(), Ok(None))
    }

    /// Tell the plugin to stop, then make sure it has. Idempotent.
    pub fn shutdown(&mut self) {
        if self.shut_down {
            return;
        }
        self.shut_down = true;

        self.send(&PluginEvent::Shutdown {
            v: PROTOCOL_VERSION,
        });
        // Ends the writer thread, which drops the plugin's stdin: the plugin
        // sees EOF even if it never looked at the shutdown event.
        self.events = None;
        if let Some(writer) = self.writer.take() {
            try_timed_join(writer, REAP_TIMEOUT);
        }

        if !self.wait_for_exit() {
            let mut child = self.locked_child();
            if let Err(error) = child.kill() {
                tracing::warn!(plugin = %self.name, %error, "cannot kill plugin process");
            }
            // Reaps the zombie; the kill above only delivers the signal.
            if let Err(error) = child.wait() {
                tracing::warn!(plugin = %self.name, %error, "cannot reap plugin process");
            }
        }

        // Both read a pipe the dead child owned, so both are at EOF by now.
        for handle in [self.reader.take(), self.stderr.take()]
            .into_iter()
            .flatten()
        {
            try_timed_join(handle, REAP_TIMEOUT);
        }
    }

    /// Poll for a clean exit within [`CHILD_EXIT_GRACE`]. `true` if it happened.
    fn wait_for_exit(&self) -> bool {
        let deadline = Instant::now() + CHILD_EXIT_GRACE;
        loop {
            match self.locked_child().try_wait() {
                Ok(Some(_)) => return true,
                // Cannot be asked about, so waiting longer will not help.
                Err(_) => return true,
                Ok(None) => {}
            }
            if Instant::now() >= deadline {
                return false;
            }
            thread::sleep(CHILD_EXIT_POLL);
        }
    }

    /// The child, recovering from a poisoned lock rather than panicking: a
    /// panic while holding it would otherwise make the process unreapable.
    fn locked_child(&self) -> std::sync::MutexGuard<'_, Child> {
        self.child.lock().unwrap_or_else(|e| e.into_inner())
    }
}

impl Drop for PluginHost {
    fn drop(&mut self) {
        self.shutdown();
    }
}

/// See [`PluginHost::spawn`] for the order and why it is that way.
fn resolve_program(command: &str, plugin_dir: Option<&Path>) -> PathBuf {
    if command.contains(std::path::MAIN_SEPARATOR) || command.contains('/') {
        return PathBuf::from(command);
    }
    if let Some(dir) = plugin_dir {
        let candidate = dir.join(command);
        if candidate.is_file() {
            return candidate;
        }
        // On Windows, an installed plugin is stored as `name.exe` but
        // configured as `name`. Try the extension before falling back to PATH.
        #[cfg(windows)]
        {
            let exe = dir.join(format!("{command}.exe"));
            if exe.is_file() {
                return exe;
            }
        }
    }
    PathBuf::from(command)
}

#[cfg(all(test, unix))]
#[path = "host_tests.rs"]
mod tests;
