//! Single entry point for platform-specific Unix socket types.
//!
//! Windows supports `AF_UNIX SOCK_STREAM`, but the standard library does not
//! expose it, so Windows goes through `uds_windows`. The APIs match, making a
//! re-export sufficient; another trait would become maintenance surface.
//!
//! This module keeps the daemon's six files from carrying their own `cfg`
//! branches, so changing socket types has one seam to update.

#[cfg(unix)]
pub(crate) use std::os::unix::net::{UnixListener, UnixStream};

#[cfg(windows)]
pub(crate) use uds_windows::{UnixListener, UnixStream};

/// Whether connecting failed because no daemon can be listening at the path.
pub(crate) fn is_unavailable(error: &std::io::Error) -> bool {
    if matches!(
        error.kind(),
        std::io::ErrorKind::NotFound | std::io::ErrorKind::ConnectionRefused
    ) {
        return true;
    }
    // macOS reports ENOTSOCK when a stale socket path has been replaced by a
    // regular file. It means the endpoint is just as unavailable as a refused
    // connection; the next daemon can remove it after taking the instance lock.
    #[cfg(unix)]
    if error.raw_os_error() == Some(libc::ENOTSOCK) {
        return true;
    }
    false
}
