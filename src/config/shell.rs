use serde::{Deserialize, Serialize};

/// The shell that every terminal pane is spawned with.
///
/// When the `[shell]` section is absent from the config, the platform default
/// is used:
///
/// | Platform | `program`                     | `command_args` |
/// |----------|-------------------------------|----------------|
/// | Unix     | `$SHELL` env var or `/bin/sh` | `["-lc"]`      |
/// | Windows  | `%ComSpec%` or `cmd.exe`      | `["/C"]`       |
///
/// `command_args` is the flag list placed *after* the shell name. The command
/// text is always the last single argv item, so the shell — not us — handles
/// its quoting/word-splitting. Interpolation like `["-c", "{}"]` is not
/// supported: that would break the contract that the shell owns quoting.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ShellConfig {
    /// Path to the shell executable. When omitted, the platform default is used.
    pub program: Option<String>,
    /// Flags passed to the shell before the command text. The command text is
    /// always the last single argv item. Default: `["-lc"]` on Unix, `["/C"]`
    /// on Windows.
    pub command_args: Vec<String>,
}

impl Default for ShellConfig {
    fn default() -> Self {
        Self {
            program: None,
            command_args: default_command_args(),
        }
    }
}

impl ShellConfig {
    /// The shell program to use, resolving the platform default when `program`
    /// is `None`.
    pub fn resolved_program(&self) -> String {
        self.program.clone().unwrap_or_else(default_program)
    }

    /// The command-line flags to place before the command text.
    pub fn command_args(&self) -> &[String] {
        &self.command_args
    }
}

/// The shell program when the user has not set one.
fn default_program() -> String {
    if cfg!(windows) {
        std::env::var("ComSpec").unwrap_or_else(|_| "cmd.exe".to_string())
    } else {
        std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".to_string())
    }
}

/// The command-line flags when the user has not set any.
fn default_command_args() -> Vec<String> {
    if cfg!(windows) {
        vec!["/C".to_string()]
    } else {
        vec!["-lc".to_string()]
    }
}
