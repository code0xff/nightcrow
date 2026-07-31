//! Waiting for the signals that mean "stop".
//!
//! A headless nightcrow has no keyboard to quit from, so the only way out is a
//! signal: Ctrl-C from the terminal it was started in (SIGINT) or a service
//! manager stopping it (SIGTERM). Both must run the same shutdown, because
//! whichever one arrives, the process owns child shells that need reaping.

use anyhow::Result;

/// A signal that asks the process to stop.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shutdown {
    /// Ctrl-C in the terminal the process was started from.
    Interrupt,
    /// `kill`, or a service manager stopping the unit.
    Terminate,
}

impl Shutdown {
    pub fn as_str(self) -> &'static str {
        match self {
            Shutdown::Interrupt => "SIGINT",
            Shutdown::Terminate => "SIGTERM",
        }
    }
}

#[cfg(unix)]
mod imp {
    use super::Shutdown;
    use anyhow::{Context, Result};
    use signal_hook::consts::signal::{SIGINT, SIGTERM};
    use signal_hook::iterator::Signals;

    pub(super) struct Watch(Signals);

    impl Watch {
        pub(super) fn register() -> Result<Self> {
            Signals::new([SIGINT, SIGTERM])
                .context("installing SIGINT/SIGTERM handlers")
                .map(Self)
        }

        pub(super) fn wait(mut self) -> Result<Shutdown> {
            for signal in self.0.forever() {
                match signal {
                    SIGINT => return Ok(Shutdown::Interrupt),
                    SIGTERM => return Ok(Shutdown::Terminate),
                    _ => {}
                }
            }
            anyhow::bail!("signal stream ended without a stop signal")
        }
    }
}

#[cfg(windows)]
mod imp {
    use super::Shutdown;
    use anyhow::{Context, Result};
    use std::sync::mpsc::{Receiver, sync_channel};

    /// Windows 는 시그널 대신 콘솔 제어 이벤트를 쓴다. 콜백을 채널로 옮겨
    /// register/wait 분리를 Unix 와 동일하게 유지한다 — 등록 시점부터 도착한
    /// 이벤트가 wait 까지 보관되어야 하고, 그게 이 계약의 요점이다.
    ///
    /// SIGTERM 대응물이 없다. Ctrl-C 와 Ctrl-Break 는 콘솔이 붙어 있을 때만
    /// 오고, `-d` 로 분리된 daemon 에는 콘솔이 없다 (detach.rs 참조).
    /// 그쪽 종료 경로는 `nightcrow stop` 이다.
    pub(super) struct Watch(Receiver<Shutdown>);

    impl Watch {
        pub(super) fn register() -> Result<Self> {
            // 깊이 1: 두 번째 Ctrl-C 는 기본 처분(즉시 종료)에 맡긴다.
            // 셧다운이 스스로 멈춰버린 경우의 탈출구가 되어야 한다 —
            // Unix 쪽 `wait` 가 self 를 consume 하는 것과 같은 의도.
            let (tx, rx) = sync_channel(1);
            ctrlc::set_handler(move || {
                let _ = tx.try_send(Shutdown::Interrupt);
            })
            .context("installing the console control handler")?;
            Ok(Self(rx))
        }

        pub(super) fn wait(self) -> Result<Shutdown> {
            self.0
                .recv()
                .context("the console control handler went away")
        }
    }
}

/// Handlers for the stop signals, installed and listening.
///
/// Registering is separate from waiting on purpose. Everything a server does
/// before it is ready to wait — binding, opening repositories, spawning
/// startup shells — takes long enough for a stop signal to arrive in the
/// middle of it, and until a handler is installed such a signal kills the
/// process at its default disposition, skipping the shutdown that reaps the
/// child shells. Register first, and a signal from that moment on is held
/// until [`ShutdownWatch::wait`] collects it.
pub struct ShutdownWatch(imp::Watch);

impl ShutdownWatch {
    pub fn register() -> Result<Self> {
        imp::Watch::register().map(Self)
    }

    /// Block until a stop signal has arrived — including one that arrived
    /// before this call, which is held from registration onward.
    ///
    /// Consumes the watch: the handlers come down with it, so a second stop
    /// signal after this returns takes its default disposition. That is
    /// deliberate — Ctrl-C during a shutdown that has itself wedged should
    /// still end the process.
    pub fn wait(self) -> Result<Shutdown> {
        self.0.wait()
    }
}

#[cfg(test)]
#[path = "signals_tests.rs"]
mod tests;
