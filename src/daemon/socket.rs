//! The daemon's Unix socket: where it lives, who may open it, and what to do
//! about one left behind by a process that is gone.
//!
//! Authentication is the filesystem. The socket sits under the user's own
//! `~/.nightcrow` at mode 0600, so reaching it already means being that user —
//! which is the same authority a client would need to run the shells the daemon
//! serves. That is why the attach path carries no password while the browser
//! path does: a TCP port is reachable by anyone who can route to it.

use anyhow::{Context, Result, bail};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::{UnixListener, UnixStream};
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

/// A bound daemon socket that unlinks itself when dropped.
#[derive(Debug)]
pub struct DaemonSocket {
    listener: UnixListener,
    path: PathBuf,
}

impl DaemonSocket {
    /// Bind the socket, refusing to displace a daemon that is already running.
    ///
    /// A socket file outliving its process is the normal case after a crash or
    /// a kill -9, and binding fails on the leftover file rather than replacing
    /// it. Telling the two apart is a connect attempt: a live daemon accepts,
    /// while a stale file refuses with `ConnectionRefused` because no process
    /// is listening on it. Only the refused one is removed — deleting a socket
    /// that answered would leave the running daemon unreachable while its
    /// clients stayed connected, which looks exactly like a hang.
    pub fn bind(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("creating the daemon directory {}", parent.display()))?;
        }
        if path.exists() {
            match UnixStream::connect(path) {
                Ok(_) => bail!(
                    "a nightcrow daemon is already running on {}",
                    path.display()
                ),
                Err(err) if err.kind() == std::io::ErrorKind::ConnectionRefused => {
                    std::fs::remove_file(path)
                        .with_context(|| format!("removing the stale socket {}", path.display()))?;
                }
                Err(err) => {
                    return Err(err).with_context(|| {
                        format!("probing the existing socket {}", path.display())
                    });
                }
            }
        }
        let listener = UnixListener::bind(path)
            .with_context(|| format!("binding the daemon socket {}", path.display()))?;
        // Narrowed after binding, which is the only order available: bind
        // creates the file. The window is between two syscalls in a directory
        // the user already owns, and the umask usually closes it first anyway.
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))
            .with_context(|| format!("restricting the daemon socket {}", path.display()))?;
        Ok(Self {
            listener,
            path: path.to_path_buf(),
        })
    }

    pub fn listener(&self) -> &UnixListener {
        &self.listener
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for DaemonSocket {
    fn drop(&mut self) {
        // Best effort: a socket file left behind is recoverable — the next bind
        // probes it, finds nothing listening, and removes it. Failing loudly
        // here would only add noise to a shutdown that is already ending.
        let _ = std::fs::remove_file(&self.path);
    }
}

#[cfg(test)]
#[path = "socket_tests.rs"]
mod tests;
