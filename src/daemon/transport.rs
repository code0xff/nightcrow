//! 플랫폼별 Unix 소켓 타입의 단일 진입점.
//!
//! Windows 도 AF_UNIX SOCK_STREAM 을 지원하지만 std 가 노출하지 않아
//! `uds_windows` 를 경유한다. 두 구현의 API 표면이 같으므로 trait 이 아니라
//! 재수출로 충분하다 — 추상화를 하나 더 만들면 그 자체가 유지 대상이 된다.
//!
//! 이 모듈을 두는 이유는 daemon 의 6개 파일이 각자 cfg 분기를 갖지 않게
//! 하는 것이다. 소켓 타입을 바꿀 일이 생기면 여기 한 곳만 본다.

#[cfg(unix)]
pub(crate) use std::os::unix::net::{UnixListener, UnixStream};

#[cfg(windows)]
pub(crate) use uds_windows::{UnixListener, UnixStream};
