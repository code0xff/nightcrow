//! Filesystem path helpers that do not belong to a domain module.

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
