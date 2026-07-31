//! 플랫폼별 Unix 소켓 타입의 단일 진입점.
#[cfg(unix)]
pub(crate) use std::os::unix::net::{UnixListener, UnixStream};
#[cfg(windows)]
pub(crate) use uds_windows::{UnixListener, UnixStream};
