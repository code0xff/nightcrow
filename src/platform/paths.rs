//! Filesystem path helpers that do not belong to a domain module.

use std::borrow::Cow;
use std::path::{Path, PathBuf};

/// Expand a leading `~` to the user's home directory.
///
/// Paths typed inside the TUI never pass through a shell, so `~/work` would
/// otherwise be taken as a directory literally named `~`. Only the bare `~`
/// form is expanded — `~user` needs a passwd lookup and is left as typed, as
/// is every path when the home directory cannot be determined.
pub(crate) fn expand_tilde(path: impl AsRef<Path>) -> PathBuf {
    let path = path.as_ref();
    // `strip_prefix` matches whole components, so this accepts `~` and
    // `~/rest` while leaving `~user/rest` alone.
    let Ok(rest) = path.strip_prefix("~") else {
        return path.to_path_buf();
    };
    match dirs::home_dir() {
        Some(home) => home.join(rest),
        None => path.to_path_buf(),
    }
}

/// Strip the verbatim prefix and normalise separators for display.
///
/// Storage and comparison use the canonical form as-is. Mixing them would
/// silently break `starts_with`-based boundary checks — git/path's worktree
/// gate depends on exactly that comparison.
pub(crate) fn for_display(path: &Path) -> Cow<'_, str> {
    let s = path.to_string_lossy();
    // `\\\\?\\` is the verbatim prefix Windows prepends to canonicalized paths.
    // Strip it so the user sees `C:\Users\...` instead of `\\?\C:\Users\...`.
    // Backslashes are also normalised to forward slashes so display paths are
    // consistent across platforms — the browser client and TUI both show `/`.
    #[cfg(windows)]
    {
        let stripped = s.strip_prefix(r"\\?\").unwrap_or(&s);
        let normalized = stripped.replace('\\', "/");
        Cow::Owned(normalized)
    }
    #[cfg(not(windows))]
    {
        s
    }
}

/// Canonicalize a path and strip the Windows verbatim prefix.
///
/// `std::fs::canonicalize` on Windows prepends `\\?\` to the result. That
/// verbatim form breaks `cmd.exe`, which treats it as a UNC path and falls
/// back to `C:\Windows`. This wrapper strips the prefix so the canonical path
/// works as a process working directory and as stored repo path. Native
/// separators are preserved — this is for filesystem/spawn use, not display.
pub(crate) fn canonicalize_clean(path: impl AsRef<Path>) -> std::io::Result<PathBuf> {
    let canonical = std::fs::canonicalize(path)?;
    #[cfg(windows)]
    {
        let s = canonical.to_string_lossy();
        let stripped = s.strip_prefix(r"\\?\").unwrap_or(&s);
        Ok(PathBuf::from(stripped))
    }
    #[cfg(not(windows))]
    {
        Ok(canonical)
    }
}

/// The directory a relative state path — the log directory, chiefly — is
/// resolved against when there is no one repository to anchor it to.
///
/// The home directory, so the default `.nightcrow/logs` lands beside the
/// config and workspace files. Not the working directory: the daemon has no
/// repository, and a client attaches from inside one it is only reading, where
/// nightcrow creates nothing. Falls back to the working directory only when
/// there is no home to find, which is the same fallback everything else here
/// takes.
pub(crate) fn state_dir_anchor() -> String {
    dirs::home_dir()
        .or_else(|| std::env::current_dir().ok())
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expand_tilde_replaces_a_leading_tilde_with_the_home_directory() {
        let home = dirs::home_dir().expect("a home directory");

        assert_eq!(expand_tilde("~/workspace/x"), home.join("workspace/x"));
    }

    #[test]
    fn expand_tilde_maps_a_bare_tilde_to_the_home_directory() {
        let home = dirs::home_dir().expect("a home directory");

        assert_eq!(expand_tilde("~"), home);
    }

    #[test]
    fn expand_tilde_leaves_paths_without_a_leading_tilde_alone() {
        assert_eq!(expand_tilde("/abs/path"), PathBuf::from("/abs/path"));
        assert_eq!(expand_tilde("rel/path"), PathBuf::from("rel/path"));
        assert_eq!(expand_tilde("/tmp/~/x"), PathBuf::from("/tmp/~/x"));
    }

    #[test]
    fn expand_tilde_leaves_a_user_qualified_tilde_alone() {
        assert_eq!(expand_tilde("~other/x"), PathBuf::from("~other/x"));
    }
}
