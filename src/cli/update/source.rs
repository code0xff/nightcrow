use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use std::process::Command;

use super::replace::replace_target;

pub(super) fn run(path: Option<&Path>, git: Option<&str>) -> Result<()> {
    let target = install_path()?;
    replace_target(&target, |_| install(path, git))
}

fn install(path: Option<&Path>, git: Option<&str>) -> Result<()> {
    let cargo = std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
    let mut command = Command::new(&cargo);
    command.arg("install").arg("--locked").arg("--force");
    match (path, git) {
        (Some(path), None) => {
            command.arg("--path").arg(path);
        }
        (None, Some(git)) => {
            command.arg("--git").arg(git).arg("nightcrow");
        }
        _ => anyhow::bail!("exactly one source must be supplied with `--path` or `--git`"),
    }

    let status = command.status().with_context(|| {
        format!(
            "could not run `{}` — source updates need a Rust toolchain on PATH",
            cargo.to_string_lossy()
        )
    })?;
    if !status.success() {
        anyhow::bail!("`cargo install` failed; nightcrow was not updated");
    }
    Ok(())
}

fn install_path() -> Result<PathBuf> {
    let root = match std::env::var_os("CARGO_INSTALL_ROOT")
        .or_else(|| std::env::var_os("CARGO_HOME"))
    {
        Some(root) => PathBuf::from(root),
        None => dirs::home_dir()
            .context("could not determine the home directory to locate the cargo bin directory")?
            .join(".cargo"),
    };
    Ok(root.join("bin").join(binary_name()))
}

fn binary_name() -> &'static str {
    if cfg!(windows) {
        "nightcrow.exe"
    } else {
        "nightcrow"
    }
}
