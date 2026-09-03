use anyhow::{Context, Result};
use std::path::Path;

use crate::platform::self_replace;

pub(super) fn replace_target(
    target: &Path,
    install: impl FnOnce(&Path) -> Result<()>,
) -> Result<()> {
    self_replace::sweep(target);
    let parked = self_replace::vacate(target).with_context(|| {
        format!(
            "could not move the installed binary at {} aside",
            target.display()
        )
    })?;
    if parked.is_some() {
        println!(
            "nightcrow: moved the installed binary aside — a running session keeps using it until it exits"
        );
    }

    match install(target).and_then(|()| {
        if target.is_file() {
            Ok(())
        } else {
            anyhow::bail!(
                "installer completed without writing the binary at {}",
                target.display()
            )
        }
    }) {
        Ok(()) => finish_success(parked.as_deref()),
        Err(err) => {
            if let Err(restore_err) = rollback(target, parked.as_deref()) {
                return Err(err.context(format!(
                    "the previous binary could not be restored: {restore_err}{}",
                    parked
                        .as_ref()
                        .map(|path| format!(" — it is at {}", path.display()))
                        .unwrap_or_default()
                )));
            }
            Err(err)
        }
    }
}

fn finish_success(parked: Option<&Path>) -> Result<()> {
    if let Some(parked) = parked
        && !self_replace::discard(parked)
    {
        println!(
            "nightcrow: the previous binary is still in use and was left at {} — it is removed on a later start",
            parked.display()
        );
    }
    println!("nightcrow: updated — restart the session to run the new version");
    Ok(())
}

fn rollback(target: &Path, parked: Option<&Path>) -> Result<()> {
    if target.exists() {
        std::fs::remove_file(target).with_context(|| {
            format!(
                "could not remove the incomplete binary at {}",
                target.display()
            )
        })?;
    }
    if let Some(parked) = parked {
        self_replace::restore(parked, target).with_context(|| {
            format!(
                "could not move the previous binary from {} back to {}",
                parked.display(),
                target.display()
            )
        })?;
    }
    Ok(())
}

#[cfg(test)]
#[path = "replace_tests.rs"]
mod tests;
