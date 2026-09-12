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
    let cleanup_pending = parked.is_some_and(|parked| !self_replace::discard(parked));
    println!("{}", success_message(cleanup_pending));
    Ok(())
}

fn success_message(cleanup_pending: bool) -> &'static str {
    if cleanup_pending {
        "nightcrow: update installed successfully.\nnightcrow: the parked old binary is still in use by the running session or updater; cleanup is pending.\nnightcrow: no second update is needed. If a session is running, run `nightcrow stop`, then start nightcrow again to use the new version."
    } else {
        "nightcrow: update installed successfully.\nnightcrow: no second update is needed. If a session is running, run `nightcrow stop`, then start nightcrow again to use the new version."
    }
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
