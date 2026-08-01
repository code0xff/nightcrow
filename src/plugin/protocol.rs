//! What the host and an out-of-process plugin say to each other.
//!
//! Newline-delimited JSON over the plugin child's stdin and stdout: the host
//! writes one [`PluginEvent`] per line, the plugin writes one
//! [`PluginCommand`] per line. A line is the framing, so nothing here may emit
//! an embedded newline.
//!
//! Unlike the daemon's control protocol, the two sides are separate builds: a
//! plugin is shipped independently against a version of this contract. That
//! makes [`PROTOCOL_VERSION`] a real negotiation — a mismatch is refused.

use crate::backend::{PaneGeneration, PaneToken};
use anyhow::{Result, anyhow, bail};
use serde::{Deserialize, Serialize};

/// Bumped when a field changes meaning. A plugin built against a version the
/// host does not speak is refused.
///
/// 2 added [`PluginCommand::WatchPane`].
pub const PROTOCOL_VERSION: u32 = 2;

/// Longest line the host will read from a plugin. Without a cap a plugin that
/// never writes a newline makes the host's reader allocate without bound.
pub const MAX_LINE_BYTES: usize = 64 * 1024;

/// Longest `data` a single [`PluginCommand::SendInput`] may carry.
///
/// Typed input stands in for a human at a keyboard. 8 KiB covers that and keeps
/// one command from filling a PTY's input buffer, which would stall every other
/// pane behind it.
pub const MAX_INPUT_BYTES: usize = 8 * 1024;

/// Sentinel [`decode_command`] answers a blank line with. Prefer
/// [`is_blank_line`] over matching this text.
const BLANK_LINE_MESSAGE: &str = "blank line carries no command";

/// Something that happened, sent host to plugin.
///
/// Every pane-scoped variant carries both the slot's token and the generation
/// within it: a plugin decides asynchronously, so the spawn it is reacting to
/// may already be gone by the time it answers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum PluginEvent {
    /// A pane slot began a spawn. `title` and `command` are absent when the
    /// host has none to report.
    PaneOpened {
        v: u32,
        token: PaneToken,
        generation: PaneGeneration,
        title: Option<String>,
        command: Option<String>,
        cwd: String,
    },
    /// Plain text the pane produced, already escape-stripped.
    PaneOutput {
        v: u32,
        token: PaneToken,
        generation: PaneGeneration,
        text: String,
    },
    /// The pane has produced nothing for this long.
    PaneIdle {
        v: u32,
        token: PaneToken,
        generation: PaneGeneration,
        idle_ms: u64,
    },
    /// The pane's process ended. The slot survives, so a relaunch is possible.
    PaneExited {
        v: u32,
        token: PaneToken,
        generation: PaneGeneration,
    },
    /// The slot itself is gone. No relaunch is possible; drop any state held
    /// for this token.
    PaneClosed {
        v: u32,
        token: PaneToken,
        generation: PaneGeneration,
    },
    /// A human typed into the pane. A plugin must treat this as a cancellation
    /// signal: the person has taken the pane back.
    UserInput {
        v: u32,
        token: PaneToken,
        generation: PaneGeneration,
    },
    /// The host is going away. Exit cleanly.
    Shutdown { v: u32 },
}

/// A request, sent plugin to host.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "cmd", rename_all = "snake_case")]
pub enum PluginCommand {
    /// Text to type into a live pane, bounded by [`MAX_INPUT_BYTES`].
    SendInput {
        v: u32,
        token: PaneToken,
        generation: PaneGeneration,
        data: String,
    },
    /// Replace an exited pane's process, appending these args to the original
    /// command. The plugin does not name the program: which binary a pane runs
    /// is the host's knowledge, and letting a plugin choose it would make this
    /// command arbitrary execution.
    Relaunch {
        v: u32,
        token: PaneToken,
        generation: PaneGeneration,
        resume_args: Vec<String>,
    },
    /// What the plugin believes about a pane. Observability only — the host
    /// displays it and acts on nothing.
    Status {
        v: u32,
        token: PaneToken,
        generation: PaneGeneration,
        state: String,
        detail: Option<String>,
        deadline_epoch: Option<i64>,
        attempt: u32,
    },
    /// Ask to be given the pane this token names, so its events start arriving.
    ///
    /// The token is the whole of the argument: it is minted per pane and reaches
    /// only that pane's child processes, so a plugin can present one merely by
    /// having been told it from inside the pane. Carries no generation — a
    /// plugin asking for a pane it has never been told about cannot know which
    /// spawn it is looking at, and the [`PluginEvent::PaneOpened`] the host
    /// answers with is what says.
    WatchPane { v: u32, token: PaneToken },
    /// A line for the host's log.
    Log {
        v: u32,
        level: LogLevel,
        message: String,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LogLevel {
    Error,
    Warn,
    Info,
    Debug,
}

impl PluginCommand {
    /// The protocol version the plugin claims to speak.
    pub fn version(&self) -> u32 {
        match self {
            Self::SendInput { v, .. }
            | Self::Relaunch { v, .. }
            | Self::Status { v, .. }
            | Self::WatchPane { v, .. }
            | Self::Log { v, .. } => *v,
        }
    }

    /// Which slot this addresses, or `None` for [`Self::Log`], which is not
    /// pane-scoped. Lets the guard layer check identity without matching every
    /// variant.
    pub fn token(&self) -> Option<&PaneToken> {
        match self {
            Self::SendInput { token, .. }
            | Self::Relaunch { token, .. }
            | Self::Status { token, .. }
            | Self::WatchPane { token, .. } => Some(token),
            Self::Log { .. } => None,
        }
    }

    /// Which spawn of the slot this addresses, paired with [`Self::token`].
    ///
    /// `None` for [`Self::WatchPane`] as well as [`Self::Log`]: naming a slot
    /// and naming a spawn within it are separate claims, and that command makes
    /// only the first.
    pub fn generation(&self) -> Option<PaneGeneration> {
        match self {
            Self::SendInput { generation, .. }
            | Self::Relaunch { generation, .. }
            | Self::Status { generation, .. } => Some(*generation),
            Self::WatchPane { .. } | Self::Log { .. } => None,
        }
    }

    /// Check the bounds serde cannot express. Called by [`decode_command`], and
    /// separately callable by anything that builds a command in-process.
    pub fn validate(&self) -> Result<()> {
        if let Self::SendInput { data, .. } = self
            && data.len() > MAX_INPUT_BYTES
        {
            bail!(
                "send_input data is {} bytes, over the {MAX_INPUT_BYTES}-byte limit",
                data.len()
            );
        }
        Ok(())
    }
}

/// Serialise one event as exactly one NDJSON line, without its terminator.
///
/// `serde_json::to_string` never emits a newline — it escapes them inside
/// strings — but the framing depends on that, so it is checked rather than
/// assumed.
pub fn encode_event(ev: &PluginEvent) -> Result<String> {
    let line = serde_json::to_string(ev).map_err(|e| anyhow!("cannot encode plugin event: {e}"))?;
    if line.contains('\n') {
        bail!("encoded plugin event contains a newline, which would split the frame");
    }
    Ok(line)
}

/// Whether a line carries no command and should be skipped.
///
/// A plugin's writer may flush a bare newline, so a blank line is expected
/// traffic rather than an error. [`decode_command`] still refuses it — it has
/// no command to return — so a reader loop tests this first.
pub fn is_blank_line(line: &str) -> bool {
    line.trim().is_empty()
}

/// Parse one line from a plugin.
///
/// Refuses an over-long line, a version this host does not speak, an unknown
/// `cmd`, and a command that fails [`PluginCommand::validate`]. A blank line is
/// refused too, with [`BLANK_LINE_MESSAGE`]; see [`is_blank_line`].
pub fn decode_command(line: &str) -> Result<PluginCommand> {
    if line.len() > MAX_LINE_BYTES {
        bail!(
            "plugin line is {} bytes, over the {MAX_LINE_BYTES}-byte limit",
            line.len()
        );
    }
    if is_blank_line(line) {
        bail!("{BLANK_LINE_MESSAGE}");
    }
    let command: PluginCommand = serde_json::from_str(line)
        .map_err(|e| anyhow!("cannot parse plugin command: {e}; unknown or malformed cmd"))?;
    if command.version() != PROTOCOL_VERSION {
        bail!(
            "plugin speaks protocol version {}, host speaks {PROTOCOL_VERSION}",
            command.version()
        );
    }
    command.validate()?;
    Ok(command)
}

#[cfg(test)]
#[path = "protocol_tests/mod.rs"]
mod tests;
