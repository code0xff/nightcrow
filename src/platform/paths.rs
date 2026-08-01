//! Filesystem path helpers that do not belong to a domain module.

use std::borrow::Cow;
use std::path::{Path, PathBuf};

#[cfg(windows)]
use std::ffi::OsString;
#[cfg(windows)]
use std::path::{Component, Prefix};

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
/// Filesystem use stays in `Path` space. Mixing display normalization into it
/// would silently break `starts_with`-based boundary checks — git/path's
/// worktree gate depends on exactly that comparison.
pub(crate) fn for_display(path: &Path) -> Cow<'_, str> {
    #[cfg(windows)]
    {
        // Convert the prefix while this is still a `Path`: going through a
        // string here would corrupt non-Unicode components, and verbatim UNC
        // needs to become `\\server\share`, not `UNC\server\share`.
        let clean = without_verbatim_prefix(path);
        Cow::Owned(clean.to_string_lossy().replace('\\', "/"))
    }
    #[cfg(not(windows))]
    {
        path.to_string_lossy()
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
        Ok(without_verbatim_prefix(&canonical).into_owned())
    }
    #[cfg(not(windows))]
    {
        Ok(canonical)
    }
}

/// Convert the two verbatim prefixes produced by Windows canonicalization to
/// paths accepted by ordinary Win32 consumers such as `cmd.exe`.
///
/// The conversion stays in `OsStr`/`Path` space so every UTF-16 code unit is
/// preserved. Other device/verbatim namespaces are left untouched: inventing
/// a non-verbatim spelling for them would change which object they name.
#[cfg(windows)]
fn without_verbatim_prefix(path: &Path) -> Cow<'_, Path> {
    let mut components = path.components();
    let Some(Component::Prefix(prefix)) = components.next() else {
        return Cow::Borrowed(path);
    };

    let mut clean = match prefix.kind() {
        Prefix::VerbatimDisk(drive) => PathBuf::from(format!("{}:\\", char::from(drive))),
        Prefix::VerbatimUNC(server, share) => {
            let mut root = OsString::from(r"\\");
            root.push(server);
            root.push(r"\");
            root.push(share);
            PathBuf::from(root)
        }
        _ => return Cow::Borrowed(path),
    };

    for component in components {
        if component != Component::RootDir {
            clean.push(component.as_os_str());
        }
    }
    Cow::Owned(clean)
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

    #[cfg(windows)]
    #[test]
    fn verbatim_drive_and_unc_paths_keep_their_native_roots() {
        assert_eq!(
            without_verbatim_prefix(Path::new(r"\\?\C:\Users\dev\repo")),
            Path::new(r"C:\Users\dev\repo")
        );
        assert_eq!(
            without_verbatim_prefix(Path::new(r"\\?\UNC\server\share\repo")),
            Path::new(r"\\server\share\repo")
        );
        assert_eq!(
            for_display(Path::new(r"\\?\UNC\server\share\repo")),
            "//server/share/repo"
        );
    }

    #[cfg(windows)]
    #[test]
    fn removing_a_verbatim_prefix_does_not_lossily_decode_components() {
        use std::os::windows::ffi::OsStringExt;

        let undecodable = OsString::from_wide(&[b'r' as u16, 0xD800, b'p' as u16]);
        let mut verbatim = PathBuf::from(r"\\?\C:\");
        verbatim.push(&undecodable);
        let mut expected = PathBuf::from(r"C:\");
        expected.push(&undecodable);

        let clean = without_verbatim_prefix(&verbatim).into_owned();
        assert_eq!(clean, expected);
    }
}
