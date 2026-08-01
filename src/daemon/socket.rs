//! The daemon's Unix socket: where it lives, who may open it, and what to do
//! about one left behind by a process that is gone.
//!
//! Authentication is the filesystem. The socket sits under the user's own
//! `~/.nightcrow` at mode 0600, so reaching it already means being that user —
//! which is the same authority a client would need to run the shells the daemon
//! serves. That is why the attach path carries no password while the browser
//! path does: a TCP port is reachable by anyone who can route to it.

use super::lock::InstanceLock;
use super::transport::UnixListener;
use anyhow::{Context, Result, bail};
use std::path::{Path, PathBuf};

/// Socket file name under the nightcrow directory.
const SOCKET_FILE: &str = "daemon.sock";

/// Default path: `~/.nightcrow/daemon.sock`.
///
/// Beside the config and workspace files rather than in `/tmp` or a runtime
/// directory: those are cleaned by the system on schedules that differ per OS,
/// and a socket that disappears under a running daemon strands every client.
pub fn default_socket_path() -> Result<PathBuf> {
    let home = dirs::home_dir().context("cannot determine the home directory")?;
    Ok(home.join(".nightcrow").join(SOCKET_FILE))
}

/// A bound daemon socket, held together with the claim that makes it ours.
///
/// Unlinks the socket when dropped; the lock is released with it.
#[derive(Debug)]
pub struct DaemonSocket {
    listener: UnixListener,
    path: PathBuf,
    /// Kept alive for its `flock`. Dropping it releases the claim, so it must
    /// outlive the listener rather than be discarded after binding.
    _lock: InstanceLock,
}

impl DaemonSocket {
    /// Bind the socket, refusing to start beside a daemon that already runs.
    ///
    /// The lock decides, not the socket file. A socket outliving its process is
    /// the normal case after a crash or a `kill -9`, and it is indistinguishable
    /// from a live one by inspection — connecting to it can even succeed. So the
    /// order is: take the lock, and only then treat whatever socket file is
    /// there as debris, because holding the lock already proves no other daemon
    /// is serving it.
    pub fn bind(path: &Path) -> Result<Self> {
        let lock_path = lock_path_for(path);
        let Some(lock) = InstanceLock::acquire(&lock_path)? else {
            bail!(
                "a nightcrow daemon is already running (holding {})",
                lock_path.display()
            );
        };
        // Safe now: the lock is ours, so nothing is listening on this path.
        if path.exists() {
            std::fs::remove_file(path)
                .with_context(|| format!("removing the stale socket {}", path.display()))?;
        }
        let listener = UnixListener::bind(path).with_context(|| {
            let len = path.as_os_str().len();
            if len >= 108 {
                format!(
                    "binding the daemon socket {} — the path is {len} bytes, over the ~107 byte AF_UNIX limit",
                    path.display()
                )
            } else {
                format!("binding the daemon socket {}", path.display())
            }
        })?;
        // Narrowed after binding, which is the only order available: bind
        // creates the file. The window is between two syscalls in a directory
        // the user already owns, and the umask usually closes it first anyway.
        restrict_to_owner(path)?;
        Ok(Self {
            listener,
            path: path.to_path_buf(),
            _lock: lock,
        })
    }

    pub fn listener(&self) -> &UnixListener {
        &self.listener
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

fn restrict_to_owner(path: &Path) -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))
            .with_context(|| format!("restricting the daemon socket {}", path.display()))?;
    }
    #[cfg(windows)]
    {
        // Windows has no mode bits — the posture depends on the directory's
        // inherited ACL. %USERPROFILE%\.nightcrow's default ACL allows write
        // only to owner and admins, so the practical posture holds.
        //
        // This dependency breaks if the socket path is placed outside the
        // user profile. Explicit ACL setting is tracked as a separate task
        // (docs/internal plan decision C).
        let _ = path;
    }
    Ok(())
}

/// The lock file guarding `socket`: the same name with a `.lock` extension, so
/// a non-default socket path brings its own lock rather than sharing one.
fn lock_path_for(socket: &Path) -> PathBuf {
    socket.with_extension("lock")
}

impl Drop for DaemonSocket {
    fn drop(&mut self) {
        // Best effort: a socket file left behind is recoverable — the next
        // daemon takes the lock and clears it. Failing loudly here would only
        // add noise to a shutdown that is already ending.
        let _ = std::fs::remove_file(&self.path);
    }
}

#[cfg(test)]
#[path = "socket_tests.rs"]
mod tests;
