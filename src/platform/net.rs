//! Socket options the standard library does not expose.
//!
//! A peer that vanishes without closing its connection leaves a socket that is
//! readable and writable forever as far as this process is concerned. The TCP
//! stack can find out, but only if keepalive probes are turned on, and std has
//! no setter for them.

use std::io;
use std::net::TcpStream;

/// Seconds a connection may sit completely idle before the first probe. Unix
/// only: Windows keeps its system-wide probe timing.
#[cfg(unix)]
pub const KEEPALIVE_IDLE_SECS: u32 = 60;
/// Seconds between probes once they start. Unix only.
#[cfg(unix)]
pub const KEEPALIVE_INTERVAL_SECS: u32 = 15;
/// Failed probes before the connection is reported dead. Unix only.
#[cfg(unix)]
pub const KEEPALIVE_PROBES: u32 = 4;

/// Turn on keepalive probing for `stream`.
///
/// On Windows only `SO_KEEPALIVE` is set: the probe timings use the system-wide
/// default there, because changing them per socket needs an ioctl and a
/// struct this crate has no other use for. The probe still turns a vanished
/// peer into a closed socket; it only takes longer than on Unix.
pub fn enable_keepalive(stream: &TcpStream) -> io::Result<()> {
    imp::enable(stream)
}

/// Whether keepalive is on for `stream`. Exists so a test can prove the option
/// reached the socket rather than merely that the call returned.
#[cfg(test)]
pub(crate) fn keepalive_enabled(stream: &TcpStream) -> io::Result<bool> {
    imp::enabled(stream)
}

#[cfg(unix)]
mod imp {
    use std::io;
    use std::net::TcpStream;
    use std::os::fd::AsRawFd;

    use super::{KEEPALIVE_IDLE_SECS, KEEPALIVE_INTERVAL_SECS, KEEPALIVE_PROBES};

    pub(super) fn enable(stream: &TcpStream) -> io::Result<()> {
        set(stream, libc::SOL_SOCKET, libc::SO_KEEPALIVE, 1)?;
        // A socket option the platform does not offer is not a reason to fail
        // the connection: keepalive is already on, and the timings only decide
        // how quickly a dead peer is noticed.
        let _ = idle(stream);
        let _ = set(
            stream,
            libc::IPPROTO_TCP,
            libc::TCP_KEEPINTVL,
            KEEPALIVE_INTERVAL_SECS as libc::c_int,
        );
        let _ = set(
            stream,
            libc::IPPROTO_TCP,
            libc::TCP_KEEPCNT,
            KEEPALIVE_PROBES as libc::c_int,
        );
        Ok(())
    }

    /// The idle period is spelled differently: `TCP_KEEPIDLE` on the kernels
    /// that follow Linux, `TCP_KEEPALIVE` on Apple's stack, and not at all on
    /// the BSDs this project does not build for — there keepalive is on, at the
    /// system's own idle period.
    #[cfg(any(target_os = "linux", target_os = "android"))]
    fn idle(stream: &TcpStream) -> io::Result<()> {
        set(
            stream,
            libc::IPPROTO_TCP,
            libc::TCP_KEEPIDLE,
            KEEPALIVE_IDLE_SECS as libc::c_int,
        )
    }

    #[cfg(any(target_os = "macos", target_os = "ios"))]
    fn idle(stream: &TcpStream) -> io::Result<()> {
        set(
            stream,
            libc::IPPROTO_TCP,
            libc::TCP_KEEPALIVE,
            KEEPALIVE_IDLE_SECS as libc::c_int,
        )
    }

    #[cfg(not(any(
        target_os = "linux",
        target_os = "android",
        target_os = "macos",
        target_os = "ios"
    )))]
    fn idle(_stream: &TcpStream) -> io::Result<()> {
        Ok(())
    }

    fn set(
        stream: &TcpStream,
        level: libc::c_int,
        name: libc::c_int,
        value: libc::c_int,
    ) -> io::Result<()> {
        let set = unsafe {
            libc::setsockopt(
                stream.as_raw_fd(),
                level,
                name,
                std::ptr::from_ref(&value).cast(),
                size_of::<libc::c_int>() as libc::socklen_t,
            )
        };
        match set {
            0 => Ok(()),
            _ => Err(io::Error::last_os_error()),
        }
    }

    #[cfg(test)]
    pub(super) fn enabled(stream: &TcpStream) -> io::Result<bool> {
        let mut value: libc::c_int = 0;
        let mut len = size_of::<libc::c_int>() as libc::socklen_t;
        let got = unsafe {
            libc::getsockopt(
                stream.as_raw_fd(),
                libc::SOL_SOCKET,
                libc::SO_KEEPALIVE,
                std::ptr::from_mut(&mut value).cast(),
                &mut len,
            )
        };
        match got {
            0 => Ok(value != 0),
            _ => Err(io::Error::last_os_error()),
        }
    }
}

#[cfg(windows)]
mod imp {
    use std::io;
    use std::net::TcpStream;
    use std::os::windows::io::AsRawSocket;
    #[cfg(test)]
    use windows_sys::Win32::Networking::WinSock::getsockopt;
    use windows_sys::Win32::Networking::WinSock::{SO_KEEPALIVE, SOCKET, SOL_SOCKET, setsockopt};

    pub(super) fn enable(stream: &TcpStream) -> io::Result<()> {
        let on: u32 = 1;
        let set = unsafe {
            setsockopt(
                stream.as_raw_socket() as SOCKET,
                SOL_SOCKET,
                SO_KEEPALIVE,
                std::ptr::from_ref(&on).cast(),
                size_of::<u32>() as i32,
            )
        };
        match set {
            0 => Ok(()),
            _ => Err(io::Error::last_os_error()),
        }
    }

    #[cfg(test)]
    pub(super) fn enabled(stream: &TcpStream) -> io::Result<bool> {
        let mut value: u32 = 0;
        let mut len = size_of::<u32>() as i32;
        let got = unsafe {
            getsockopt(
                stream.as_raw_socket() as SOCKET,
                SOL_SOCKET,
                SO_KEEPALIVE,
                std::ptr::from_mut(&mut value).cast(),
                &mut len,
            )
        };
        match got {
            0 => Ok(value != 0),
            _ => Err(io::Error::last_os_error()),
        }
    }
}

#[cfg(test)]
#[path = "net_tests.rs"]
mod tests;
