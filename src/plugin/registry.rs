//! The on-disk set of installed plugin executables (`~/.nightcrow/plugins`).
//!
//! Installing a binary here does not switch it on: the host only ever launches
//! a plugin that `config.toml` declares in a `[[plugin]]` table *and* that some
//! pane can reach — either a `[[startup_command]]` opted one in by name, or
//! `watch_on_signal` lets a provider signal name one. That edit is left to the
//! user on purpose — a plugin can drive a pane's terminal, so the file that
//! grants it that has to be one a person read.
//!
//! Every function takes the plugins directory as a parameter so the filesystem
//! behaviour is testable against a temp directory.

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

/// Subdirectory of `~/.nightcrow` holding installed plugin executables.
const PLUGINS_SUBDIR: &str = "plugins";
/// Owner-only read/write/execute for an installed plugin. Least privilege, and
/// the same posture the config file's 0600 takes: nothing else on the machine
/// has business running a binary nightcrow will attach to a terminal.
#[cfg(unix)]
const PLUGIN_MODE: u32 = 0o700;
/// Longest accepted plugin name.
const MAX_NAME_LEN: usize = 64;

/// `~/.nightcrow/plugins`, resolved whether or not it exists yet. Errors only
/// when the home directory cannot be determined.
pub fn default_plugins_dir() -> Result<PathBuf> {
    let home = dirs::home_dir().context("cannot determine the home directory")?;
    Ok(home.join(".nightcrow").join(PLUGINS_SUBDIR))
}

/// Result of [`install`], so the caller can report exactly what was touched.
#[derive(Debug)]
pub enum InstallOutcome {
    Created(PathBuf),
    Replaced(PathBuf),
    /// A plugin of that name was already installed and `force` was not set.
    AlreadyExists(PathBuf),
}

/// Result of [`remove`]. Removing something absent is a report, not a failure.
#[derive(Debug)]
pub enum RemoveOutcome {
    Removed(PathBuf),
    NotInstalled(String),
}

/// How `config.toml` currently refers to an installed plugin.
#[derive(Debug, PartialEq, Eq)]
pub struct PluginStatus {
    pub declared: bool,
    pub enabled: bool,
    /// `[[startup_command]]` entries whose `plugin =` names this plugin.
    pub opt_ins: usize,
}

/// Accept only a safe single filename.
///
/// This is the path-traversal boundary: the name is joined onto the plugins
/// directory and then written to and deleted, so anything that could escape
/// that directory — a separator, `.`/`..` — is refused here rather than
/// sanitised. A leading `-` is refused too, because such a file name is read as
/// a flag by every command the user might later point at it.
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
            .all(|b| b.is_ascii_alphanumeric() || b == b'.' || b == b'_' || b == b'-'),
        "plugin name \"{name}\" may only contain letters, digits, '.', '_' and '-'"
    );
    Ok(())
}

/// Copy the executable at `source` into `base` under `name` (defaulting to the
/// source file stem) and restrict it to owner-only.
pub fn install(
    base: &Path,
    source: &Path,
    name: Option<&str>,
    force: bool,
) -> Result<InstallOutcome> {
    let name = match name {
        Some(name) => name.to_string(),
        None => derive_name(source)?,
    };
    validate_name(&name)?;
    check_source(source)?;

    let dest = base.join(&name);
    let installed = dest.symlink_metadata().is_ok();
    if installed && !force {
        return Ok(InstallOutcome::AlreadyExists(dest));
    }
    std::fs::create_dir_all(base)
        .with_context(|| format!("creating plugin directory {}", base.display()))?;
    // Unlink rather than copy over: truncating a binary that is currently
    // running fails with ETXTBSY, and a fresh inode leaves any live process
    // on the old file.
    if installed {
        std::fs::remove_file(&dest)
            .with_context(|| format!("replacing installed plugin {}", dest.display()))?;
    }
    std::fs::copy(source, &dest)
        .with_context(|| format!("copying plugin {} to {}", source.display(), dest.display()))?;
    restrict_permissions(&dest)?;
    Ok(if installed {
        InstallOutcome::Replaced(dest)
    } else {
        InstallOutcome::Created(dest)
    })
}

/// Installed plugin names, sorted. A missing directory lists as empty — that is
/// the state before the first install, not an error.
pub fn list(base: &Path) -> Result<Vec<String>> {
    let entries = match std::fs::read_dir(base) {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => {
            return Err(e).with_context(|| format!("reading plugin directory {}", base.display()));
        }
    };
    let mut names = Vec::new();
    for entry in entries {
        let entry =
            entry.with_context(|| format!("reading plugin directory {}", base.display()))?;
        // Follows symlinks, so a hand-linked plugin still lists.
        if !std::fs::metadata(entry.path()).is_ok_and(|m| m.is_file()) {
            continue;
        }
        names.push(entry.file_name().to_string_lossy().into_owned());
    }
    names.sort();
    Ok(names)
}

/// Delete an installed plugin.
pub fn remove(base: &Path, name: &str) -> Result<RemoveOutcome> {
    validate_name(name)?;
    let path = base.join(name);
    if path.symlink_metadata().is_err() {
        return Ok(RemoveOutcome::NotInstalled(name.to_string()));
    }
    std::fs::remove_file(&path)
        .with_context(|| format!("removing installed plugin {}", path.display()))?;
    Ok(RemoveOutcome::Removed(path))
}

/// What the loaded config says about `name`.
pub fn status(cfg: &crate::config::Config, name: &str) -> PluginStatus {
    let declared = cfg.plugins.iter().find(|p| p.name == name);
    PluginStatus {
        declared: declared.is_some(),
        enabled: declared.is_some_and(|p| p.enabled),
        opt_ins: cfg
            .startup_commands
            .iter()
            .filter(|sc| sc.plugin.as_deref() == Some(name))
            .count(),
    }
}

/// The `[[plugin]]` block to paste into `config.toml` after an install.
///
/// Printed, never written: enabling a plugin hands it a pane's terminal, so the
/// opt-in stays an explicit edit the user reviews. `enabled = false` and an
/// empty `allowed_resume_flags` are the off positions of both switches.
pub fn config_snippet(name: &str, command: &Path) -> String {
    format!(
        "[[plugin]]\n\
         name = \"{name}\"\n\
         command = \"{}\"\n\
         args = []\n\
         allowed_resume_flags = []\n\
         enabled = false\n",
        toml_escape(&command.display().to_string())
    )
}

fn toml_escape(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

fn derive_name(source: &Path) -> Result<String> {
    source
        .file_stem()
        .map(|stem| stem.to_string_lossy().into_owned())
        .ok_or_else(|| {
            anyhow::anyhow!(
                "cannot derive a plugin name from {}; pass --name",
                source.display()
            )
        })
}

fn check_source(source: &Path) -> Result<()> {
    let meta = std::fs::symlink_metadata(source).with_context(|| {
        format!(
            "plugin source {} cannot be read; it must be an existing executable file",
            source.display()
        )
    })?;
    let meta = if meta.file_type().is_symlink() {
        std::fs::metadata(source)
            .with_context(|| format!("plugin source {} is a broken symlink", source.display()))?
    } else {
        meta
    };
    anyhow::ensure!(
        meta.is_file(),
        "plugin source {} is not a regular file",
        source.display()
    );
    anyhow::ensure!(
        is_executable(source),
        "plugin source {} is not executable by the current user; chmod +x it first",
        source.display()
    );
    Ok(())
}

#[cfg(unix)]
fn is_executable(path: &Path) -> bool {
    use std::os::unix::ffi::OsStrExt;
    let Ok(c_path) = std::ffi::CString::new(path.as_os_str().as_bytes()) else {
        return false;
    };
    // access(2) rather than a permission-bit test: it answers the question that
    // matters — whether *this* user may execute it — for owner, group and other
    // in one call.
    unsafe { libc::access(c_path.as_ptr(), libc::X_OK) == 0 }
}

#[cfg(not(unix))]
fn is_executable(_path: &Path) -> bool {
    true
}

fn restrict_permissions(path: &Path) -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(PLUGIN_MODE))
            .with_context(|| format!("restricting permissions on {}", path.display()))?;
    }
    #[cfg(not(unix))]
    {
        let _ = path;
    }
    Ok(())
}

#[cfg(test)]
#[path = "registry_tests.rs"]
mod tests;
