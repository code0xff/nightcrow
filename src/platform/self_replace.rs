//! Making room for a new binary where the old one is still running.
//!
//! Windows refuses to overwrite a running executable (`os error 5`) but allows
//! renaming it, since a rename touches only the directory entry. So an update
//! parks the installed binary under a sibling name, leaves the path free for
//! the installer, and deletes the parked copy after. A delete blocked by the
//! still-running process is retried by [`sweep`] on a later startup.
//!
//! Not `cfg`-gated: rename-then-delete is correct everywhere, so one code path
//! serves all platforms and the tests cover it on all of them.

use std::io;
use std::path::{Path, PathBuf};

/// Distinctive enough that [`sweep`] only ever deletes files this module made.
const PARKED_SUFFIX: &str = ".nightcrow-old";

/// Each still-locked leftover takes a slot; more than a handful is a bug.
const MAX_PARKED_SLOTS: u32 = 32;

/// Move `path` aside so an installer can write a fresh binary there.
///
/// Returns the parked path, or `None` if `path` does not exist. The caller must
/// then [`discard`] it on a successful install or [`restore`] it on a failed one.
pub(crate) fn vacate(path: &Path) -> io::Result<Option<PathBuf>> {
    if !path.exists() {
        return Ok(None);
    }
    for slot in 0..MAX_PARKED_SLOTS {
        let parked = parked_path(path, slot);
        // A leftover here is still running, and renaming onto a locked file
        // fails like writing over one. Reuse the slot only if it frees up.
        if parked.exists() && std::fs::remove_file(&parked).is_err() {
            continue;
        }
        std::fs::rename(path, &parked)?;
        return Ok(Some(parked));
    }
    Err(io::Error::other(format!(
        "could not move {} aside: every parked slot is taken by a binary that is still in use",
        path.display()
    )))
}

/// Undo a [`vacate`], so a failed install leaves the old version in place.
pub(crate) fn restore(parked: &Path, path: &Path) -> io::Result<()> {
    std::fs::rename(parked, path)
}

/// Delete a parked binary. Failing is normal — it stays locked while the
/// process that started from it runs, and [`sweep`] gets it later.
pub(crate) fn discard(parked: &Path) -> bool {
    std::fs::remove_file(parked).is_ok()
}

/// Give a downloaded replacement the permissions expected of an executable.
pub(crate) fn make_executable(path: &Path) -> io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        let mut permissions = std::fs::metadata(path)?.permissions();
        permissions.set_mode(0o755);
        std::fs::set_permissions(path, permissions)?;
    }
    #[cfg(not(unix))]
    let _ = path;
    Ok(())
}

/// Delete binaries parked beside `path` by earlier updates.
///
/// Best-effort: this runs on startup, where a leftover costs a few megabytes
/// and is never a reason to refuse to start.
pub(crate) fn sweep(path: &Path) {
    let Some(dir) = path.parent() else { return };
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let Some(name) = name.to_str() else { continue };
        if !is_parked_name(name) {
            continue;
        }
        match std::fs::remove_file(entry.path()) {
            Ok(()) => tracing::debug!(file = name, "swept a binary parked by an earlier update"),
            // Still running, or not ours to delete. Next startup tries again.
            Err(err) => tracing::debug!(%err, file = name, "parked binary is still in use"),
        }
    }
}

/// The startup hook: whatever the last update could not delete, this gets.
pub(crate) fn sweep_beside_current_exe() {
    if let Ok(exe) = std::env::current_exe() {
        sweep(&exe);
    }
}

fn parked_path(path: &Path, slot: u32) -> PathBuf {
    let name = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| "nightcrow".to_owned());
    path.with_file_name(format!("{name}{PARKED_SUFFIX}.{slot}"))
}

/// The slot number is required, so a file that merely ends in the suffix is
/// not mistaken for ours.
fn is_parked_name(name: &str) -> bool {
    let Some((head, slot)) = name.rsplit_once('.') else {
        return false;
    };
    head.ends_with(PARKED_SUFFIX) && !slot.is_empty() && slot.bytes().all(|b| b.is_ascii_digit())
}

#[cfg(test)]
#[path = "self_replace_tests.rs"]
mod tests;
