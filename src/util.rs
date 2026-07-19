//! Shared low-level utilities. Keep tiny — anything domain-specific should
//! live in the relevant module instead.

use std::path::{Path, PathBuf};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

/// Expand a leading `~` to the user's home directory.
///
/// Paths typed inside the TUI never pass through a shell, so `~/work` would
/// otherwise be taken as a directory literally named `~`. Only the bare `~`
/// form is expanded — `~user` needs a passwd lookup and is left as typed, as
/// is every path when the home directory cannot be determined.
pub fn expand_tilde(path: impl AsRef<Path>) -> PathBuf {
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

/// Default reap window for [`try_timed_join`] at known-quiescent call sites
/// (Drop impls, worker swap-out). The signal-then-join pattern means the
/// worker is already a few syscalls from returning; a handful of millis is
/// generous without ever stalling the UI noticeably.
pub const REAP_TIMEOUT: Duration = Duration::from_millis(5);

/// Spin briefly waiting for `handle` to finish, then either join it or
/// detach the handle. Detaches without panicking on timeout so the UI is
/// never blocked by a hung worker. Used at quiescent moments (repo switch,
/// after reply drain, Drop) where the worker is either already done or
/// microseconds away from exiting.
pub fn try_timed_join(handle: JoinHandle<()>, timeout: Duration) {
    let deadline = Instant::now() + timeout;
    while !handle.is_finished() && Instant::now() < deadline {
        // Short sleep keeps the busy-wait cheap; the common path is one
        // iteration because the worker exits as soon as it tries to send.
        thread::sleep(Duration::from_micros(200));
    }
    if handle.is_finished() {
        if let Err(e) = handle.join() {
            tracing::warn!(?e, "worker thread panicked");
        }
    } else {
        tracing::debug!("worker still running at detach; reaping deferred to OS");
        drop(handle);
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
        // A tilde that is not the first component is an ordinary directory
        // name, not a home reference.
        assert_eq!(expand_tilde("/tmp/~/x"), PathBuf::from("/tmp/~/x"));
    }

    #[test]
    fn expand_tilde_leaves_a_user_qualified_tilde_alone() {
        // `~other` needs a passwd lookup; expanding it against our own home
        // would silently point at the wrong directory.
        assert_eq!(expand_tilde("~other/x"), PathBuf::from("~other/x"));
    }
}
