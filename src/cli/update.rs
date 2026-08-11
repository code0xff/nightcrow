//! `nightcrow update` — reinstall nightcrow over the copy that is running.
//!
//! The install is `cargo install`, the same command the README gives. What this
//! adds is moving the installed binary aside first, so the install has a free
//! path to write to on Windows instead of failing with a permission error —
//! see [`crate::platform::self_replace`].

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::platform::self_replace;

/// The source when none is given.
const DEFAULT_GIT_URL: &str = "https://github.com/code0xff/nightcrow";

/// Reinstall nightcrow from `path` if given, else from `git` (defaulting to
/// the upstream repository).
pub(crate) fn run_update(path: Option<PathBuf>, git: Option<String>) -> Result<()> {
    // Free the slots earlier updates left behind, so repeated updates on a
    // machine that always has a session running do not exhaust them.
    let target = install_path()?;
    self_replace::sweep(&target);

    let parked = self_replace::vacate(&target).with_context(|| {
        format!(
            "could not move the installed binary at {} aside to make room for the new one",
            target.display()
        )
    })?;
    if parked.is_some() {
        println!(
            "nightcrow: moved the installed binary aside — a running session keeps using it until it exits"
        );
    }

    match install(path.as_deref(), git.as_deref()) {
        Ok(()) => {
            if let Some(parked) = parked
                && !self_replace::discard(&parked)
            {
                // Expected while a session is up. Startup sweeps it later.
                println!(
                    "nightcrow: the previous binary is still in use and was left at {} — it is removed on a later start",
                    parked.display()
                );
            }
            println!("nightcrow: updated — restart the session to run the new version");
            Ok(())
        }
        Err(err) => {
            // Leave the old version in place rather than no version at all.
            if let Some(parked) = parked
                && let Err(restore_err) = self_replace::restore(&parked, &target)
            {
                return Err(err.context(format!(
                    "the previous binary could not be put back either: {restore_err} — it is at {}",
                    parked.display()
                )));
            }
            Err(err)
        }
    }
}

/// `--locked` matches the documented install, so an update resolves the same
/// dependency versions. `--force` is required: without it cargo can decide the
/// package is already installed and skip, leaving the vacated path empty.
fn install(path: Option<&Path>, git: Option<&str>) -> Result<()> {
    let cargo = std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
    let mut command = Command::new(&cargo);
    command.arg("install").arg("--locked").arg("--force");
    match path {
        Some(path) => {
            command.arg("--path").arg(path);
        }
        None => {
            command
                .arg("--git")
                .arg(git.unwrap_or(DEFAULT_GIT_URL))
                .arg("nightcrow");
        }
    }

    let status = command.status().with_context(|| {
        format!(
            "could not run `{}` — updating needs a Rust toolchain on PATH",
            cargo.to_string_lossy()
        )
    })?;
    if !status.success() {
        anyhow::bail!("`cargo install` failed; nightcrow was not updated");
    }
    Ok(())
}

/// The path `cargo install` will write to — the one that has to be vacated.
/// Not the running executable: a copy living elsewhere is not in the
/// installer's way, and in the case that fails the two are the same path.
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
