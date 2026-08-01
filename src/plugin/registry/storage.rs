use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

use super::executable::{restrict_permissions, validate_source};
use super::{InstallOutcome, RemoveOutcome, validate_name};

const PLUGINS_SUBDIR: &str = "plugins";

/// Resolve `~/.nightcrow/plugins` without creating it.
pub fn default_plugins_dir() -> Result<PathBuf> {
    let home = dirs::home_dir().context("cannot determine the home directory")?;
    Ok(home.join(".nightcrow").join(PLUGINS_SUBDIR))
}

/// Copy an executable into the registry and restrict it to the owner.
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
    validate_source(source)?;

    let destination = base.join(&name);
    let installed = destination.symlink_metadata().is_ok();
    if installed && !force {
        return Ok(InstallOutcome::AlreadyExists(destination));
    }
    std::fs::create_dir_all(base)
        .with_context(|| format!("creating plugin directory {}", base.display()))?;
    // A new inode avoids ETXTBSY when the installed binary is still running.
    if installed {
        std::fs::remove_file(&destination)
            .with_context(|| format!("replacing installed plugin {}", destination.display()))?;
    }
    std::fs::copy(source, &destination).with_context(|| {
        format!(
            "copying plugin {} to {}",
            source.display(),
            destination.display()
        )
    })?;
    restrict_permissions(&destination)?;

    Ok(if installed {
        InstallOutcome::Replaced(destination)
    } else {
        InstallOutcome::Created(destination)
    })
}

/// Installed plugin names in sorted order; a missing registry is empty.
pub fn list(base: &Path) -> Result<Vec<String>> {
    let entries = match std::fs::read_dir(base) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => {
            return Err(error)
                .with_context(|| format!("reading plugin directory {}", base.display()));
        }
    };
    let mut names = Vec::new();
    for entry in entries {
        let entry =
            entry.with_context(|| format!("reading plugin directory {}", base.display()))?;
        if !std::fs::metadata(entry.path()).is_ok_and(|metadata| metadata.is_file()) {
            continue;
        }
        let raw = entry.file_name().to_string_lossy().into_owned();
        #[cfg(windows)]
        let name = raw.strip_suffix(".exe").map(str::to_string).unwrap_or(raw);
        #[cfg(not(windows))]
        let name = raw;
        names.push(name);
    }
    names.sort();
    Ok(names)
}

/// Remove a plugin or report that it was not installed.
pub fn remove(base: &Path, name: &str) -> Result<RemoveOutcome> {
    validate_name(name)?;
    let path = resolve_installed_path(base, name);
    if path.symlink_metadata().is_err() {
        return Ok(RemoveOutcome::NotInstalled(name.to_string()));
    }
    std::fs::remove_file(&path)
        .with_context(|| format!("removing installed plugin {}", path.display()))?;
    Ok(RemoveOutcome::Removed(path))
}

fn resolve_installed_path(base: &Path, name: &str) -> PathBuf {
    let bare = base.join(name);
    if bare.symlink_metadata().is_ok() {
        return bare;
    }
    #[cfg(windows)]
    {
        let executable = base.join(format!("{name}.exe"));
        if executable.symlink_metadata().is_ok() {
            return executable;
        }
    }
    bare
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
