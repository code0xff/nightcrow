//! The wire contract for an out-of-process plugin.
//!
//! Trust posture: a plugin is a separate program the host launches and speaks
//! NDJSON to, so everything arriving from it is untrusted input. Every command
//! is validated by the host — version, line length, payload bounds, and the
//! pane identity it names — before anything acts on it. This module is types
//! and parsing only: no IO, no process spawning, no threads.
//!
//! Which panes a plugin may see is settled here too. It is normally the
//! operator's list, and a plugin can add to it only by quoting a pane's own
//! token back — something only a process inside that pane can know — and only
//! where the operator turned that on. A plugin is never handed a list of panes
//! to choose from.
//!
//! The contract is deliberately provider-agnostic. It speaks of panes, output,
//! idleness and relaunching, and never of any particular tool; which program a
//! pane runs stays the host's knowledge, which is what keeps
//! [`protocol::PluginCommand::Relaunch`] from being arbitrary execution.

//! The layering is deliberate: `protocol` is types only, `host` moves bytes and
//! knows nothing about authority, and `guard` decides authority and touches no
//! IO. Nothing acts on a plugin's command without passing through `guard`.

pub mod guard;
pub mod host;
pub mod protocol;
pub mod registry;

mod guard_budget;
mod guard_refusal;
mod guard_text;
mod guard_watch;
mod host_command;
mod host_pump;

pub use guard::{Approved, Guard, PaneFacts};
pub use guard_budget::{RateAction, RateLimits};
pub use guard_refusal::Refused;
pub use guard_text::MAX_LOG_MESSAGE_BYTES;
pub use host::PluginHost;
