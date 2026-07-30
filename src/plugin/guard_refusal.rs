//! Why a plugin's command was not carried out.
//!
//! Each variant carries what a log line needs to be actionable without the
//! reader having to correlate it with anything else: which pane, and the values
//! the rule compared.

use super::guard_budget::RateAction;
use crate::backend::{PaneGeneration, PaneId, PaneToken};
use std::fmt;
use std::time::Duration;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Refused {
    /// The token names no pane the host still has.
    UnknownPane { token: PaneToken },
    /// The pane exists but its `[[startup_command]]` did not name this plugin.
    NotOptedIn { pane: PaneId, token: PaneToken },
    /// The command is about a process that has already been replaced.
    StaleGeneration {
        pane: PaneId,
        claimed: PaneGeneration,
        current: PaneGeneration,
    },
    /// Input for a pane whose process has exited.
    PaneNotRunning { pane: PaneId },
    /// Input for a pane that is still producing output.
    PaneBusy {
        pane: PaneId,
        idle: Duration,
        min_idle: Duration,
    },
    InputTooLarge {
        pane: PaneId,
        bytes: usize,
        limit: usize,
    },
    /// Input holding a control character that is not `\r`, `\n`, or `\t`.
    ControlCharacter { pane: PaneId, code: u32 },
    /// A relaunch for a pane whose process is still running.
    PaneStillRunning { pane: PaneId },
    /// A relaunch for a pane the host never launched a command for.
    ///
    /// Its own reason rather than one of `ResumeArgsRejected`'s, because nothing
    /// about the arguments is wrong: the pane is a bare shell, so there is no
    /// invocation to append them to and no amount of retrying changes that.
    NoLaunchCommand { pane: PaneId },
    /// The resume arguments did not survive the command-line rules.
    ResumeArgsRejected { pane: PaneId, reason: String },
    /// A plugin asked for a pane it was never named by, without the config
    /// switch that allows it.
    WatchNotAllowed { pane: PaneId, token: PaneToken },
    /// A plugin asked for a pane another plugin already has.
    PaneWatchedByAnother { pane: PaneId },
    RateLimited {
        pane: PaneId,
        action: RateAction,
        limit: u32,
        window: Duration,
    },
}

impl fmt::Display for Refused {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownPane { token } => {
                write!(f, "no live pane for token {}", token.as_str())
            }
            Self::NotOptedIn { pane, token } => write!(
                f,
                "pane {pane} (token {}) did not opt in to this plugin",
                token.as_str()
            ),
            Self::StaleGeneration {
                pane,
                claimed,
                current,
            } => write!(
                f,
                "pane {pane} is on generation {current}, command claims {claimed}"
            ),
            Self::PaneNotRunning { pane } => {
                write!(f, "pane {pane} has no running process to type into")
            }
            Self::PaneBusy {
                pane,
                idle,
                min_idle,
            } => write!(
                f,
                "pane {pane} has been idle {idle:?}, under the {min_idle:?} required"
            ),
            Self::InputTooLarge { pane, bytes, limit } => {
                write!(f, "input for pane {pane} is {bytes} bytes, over {limit}")
            }
            Self::ControlCharacter { pane, code } => write!(
                f,
                "input for pane {pane} holds control character U+{code:04X}"
            ),
            Self::PaneStillRunning { pane } => {
                write!(
                    f,
                    "pane {pane} is still running, so it cannot be relaunched"
                )
            }
            Self::NoLaunchCommand { pane } => write!(
                f,
                "pane {pane} was launched with no command, so there is nothing to relaunch"
            ),
            Self::ResumeArgsRejected { pane, reason } => {
                write!(f, "relaunch of pane {pane} refused: {reason}")
            }
            Self::WatchNotAllowed { pane, token } => write!(
                f,
                "pane {pane} (token {}) was not opted in, and this plugin's config \
                 does not let it watch a pane on a signal",
                token.as_str()
            ),
            Self::PaneWatchedByAnother { pane } => {
                write!(f, "pane {pane} is already watched by another plugin")
            }
            Self::RateLimited {
                pane,
                action,
                limit,
                window,
            } => write!(
                f,
                "pane {pane} already used its {limit} {} per {window:?}",
                action.as_str()
            ),
        }
    }
}
