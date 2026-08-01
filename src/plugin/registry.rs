//! Installed plugin executables and their explicit config state.
//!
//! Installation only places a binary in `~/.nightcrow/plugins`. A plugin stays
//! inert until the user declares it in config and opts a pane into it.

use anyhow::Result;
use std::path::PathBuf;

#[cfg(test)]
use std::path::Path;

mod config;
mod executable;
mod storage;

pub use config::{config_snippet, status};
pub use storage::{default_plugins_dir, install, list, remove};

const MAX_NAME_LEN: usize = 64;

/// Result of placing a plugin executable in the registry.
#[derive(Debug)]
pub enum InstallOutcome {
    Created(PathBuf),
    Replaced(PathBuf),
    /// The destination existed and replacement was not requested.
    AlreadyExists(PathBuf),
}

/// Result of removing a plugin; absence is a report, not an error.
#[derive(Debug)]
pub enum RemoveOutcome {
    Removed(PathBuf),
    NotInstalled(String),
}

/// How the loaded config refers to an installed plugin.
#[derive(Debug, PartialEq, Eq)]
pub struct PluginStatus {
    pub declared: bool,
    pub enabled: bool,
    /// `[[startup_command]]` entries whose `plugin =` names this plugin.
    pub opt_ins: usize,
}

/// Enforce the single-filename boundary used by install and remove.
pub fn validate_name(name: &str) -> Result<()> {
    anyhow::ensure!(!name.is_empty(), "a plugin name must not be empty");
    anyhow::ensure!(
        name.len() <= MAX_NAME_LEN,
        "plugin name \"{name}\" is longer than {MAX_NAME_LEN} characters"
    );
    anyhow::ensure!(
        !name.contains('/') && !name.contains('\\'),
        "plugin name \"{name}\" must be a single file name, not a path"
    );
    anyhow::ensure!(
        name != "." && name != "..",
        "plugin name \"{name}\" is a directory reference, not a file name"
    );
    anyhow::ensure!(
        !name.starts_with('-'),
        "plugin name \"{name}\" must not start with '-'; such a name is read as a flag"
    );
    anyhow::ensure!(
        name.bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-')),
        "plugin name \"{name}\" may only contain letters, digits, '.', '_' and '-'"
    );
    Ok(())
}

#[cfg(test)]
#[path = "registry_tests.rs"]
mod tests;
