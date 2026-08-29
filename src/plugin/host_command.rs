//! Build the platform-specific command used to launch a plugin.

use crate::config::PluginConfig;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// Resolve and configure a plugin command before it is spawned.
pub(super) fn configure_command(
    cfg: &PluginConfig,
    plugin_dir: Option<&Path>,
    runtime_dir: Option<&Path>,
) -> (PathBuf, Command) {
    let program = resolve_program(&cfg.command, plugin_dir);
    let mut command = Command::new(&program);
    command
        .args(&cfg.args)
        .envs(&cfg.env)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    // After `cfg.env`: which hub a plugin belongs to is the host's to say,
    // not something a config can point at another hub's socket. See
    // `PLUGIN_RUNTIME_DIR_ENV`.
    if let Some(dir) = runtime_dir {
        command.env(crate::backend::identity::PLUGIN_RUNTIME_DIR_ENV, dir);
    }
    no_console_window(&mut command);
    (program, command)
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

/// See [`super::host::PluginHost::spawn`] for the order and why it is that way.
fn resolve_program(command: &str, plugin_dir: Option<&Path>) -> PathBuf {
    if command.contains(std::path::MAIN_SEPARATOR) || command.contains('/') {
        return PathBuf::from(command);
    }
    if let Some(dir) = plugin_dir {
        let candidate = dir.join(command);
        if candidate.is_file() {
            return candidate;
        }
        // On Windows an installed plugin is stored as `name.exe` but
        // configured as `name`; try the extension before PATH.
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
