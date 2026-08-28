//! The plugin's side of nightcrow's NDJSON plugin contract.
//!
//! Deliberately a standalone copy of the host's `src/plugin/protocol.rs` rather
//! than a shared crate: a plugin is built and shipped separately from the
//! host, so it is written against a *version* of the contract.
//! [`PROTOCOL_VERSION`] is what makes a mismatch loud instead of
//! half-understood, and a copy is what makes the version claim honest.

use serde::{Deserialize, Serialize};

/// Contract version this plugin speaks. The host refuses anything else.
///
/// 2 is the first version with [`PluginCommand::WatchPane`], which this plugin
/// needs: a pane somebody started a provider CLI in by hand is never named to
/// us, so asking for it is the only way to watch it at all.
pub const PROTOCOL_VERSION: u32 = 3;

/// Longest line the host will read from us; also the cap we apply to what we
/// read, so a corrupt stream cannot make this process allocate without bound.
pub const MAX_LINE_BYTES: usize = 64 * 1024;

/// Longest `data` the host accepts in one [`PluginCommand::SendInput`].
pub const MAX_INPUT_BYTES: usize = 8 * 1024;

/// Opaque pane-slot name. Random hex minted by the host; we only ever compare
/// and forward it.
pub type PaneToken = String;

/// Which spawn of a pane slot an event or command refers to.
pub type PaneGeneration = u32;

/// Env var carrying the pane token into the pane's child processes, and hence
/// into a provider CLI's hook and statusline helpers. That inheritance is how
/// an out-of-band signal is attributed to a pane; cwd cannot do it, because
/// nightcrow allows several panes on one repository.
pub const PANE_TOKEN_ENV: &str = "NIGHTCROW_PANE_TOKEN";

/// Something the host observed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum PluginEvent {
    PaneOpened {
        v: u32,
        token: PaneToken,
        generation: PaneGeneration,
        title: Option<String>,
        command: Option<String>,
        cwd: String,
    },
    PaneOutput {
        v: u32,
        token: PaneToken,
        generation: PaneGeneration,
        text: String,
    },
    PaneIdle {
        v: u32,
        token: PaneToken,
        generation: PaneGeneration,
        idle_ms: u64,
    },
    PaneExited {
        v: u32,
        token: PaneToken,
        generation: PaneGeneration,
    },
    PaneClosed {
        v: u32,
        token: PaneToken,
        generation: PaneGeneration,
    },
    UserInput {
        v: u32,
        token: PaneToken,
        generation: PaneGeneration,
    },
    Shutdown {
        v: u32,
    },
}

impl PluginEvent {
    /// Which slot this addresses, or `None` for [`Self::Shutdown`].
    pub fn token(&self) -> Option<&PaneToken> {
        match self {
            Self::PaneOpened { token, .. }
            | Self::PaneOutput { token, .. }
            | Self::PaneIdle { token, .. }
            | Self::PaneExited { token, .. }
            | Self::PaneClosed { token, .. }
            | Self::UserInput { token, .. } => Some(token),
            Self::Shutdown { .. } => None,
        }
    }

    pub fn generation(&self) -> Option<PaneGeneration> {
        match self {
            Self::PaneOpened { generation, .. }
            | Self::PaneOutput { generation, .. }
            | Self::PaneIdle { generation, .. }
            | Self::PaneExited { generation, .. }
            | Self::PaneClosed { generation, .. }
            | Self::UserInput { generation, .. } => Some(*generation),
            Self::Shutdown { .. } => None,
        }
    }
}

/// Something we ask the host to do. The host judges every one of these and
/// refusal is ordinary traffic, never a reason to retry in a loop.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "cmd", rename_all = "snake_case")]
pub enum PluginCommand {
    SendInput {
        v: u32,
        token: PaneToken,
        generation: PaneGeneration,
        data: String,
    },
    Relaunch {
        v: u32,
        token: PaneToken,
        generation: PaneGeneration,
        resume_args: Vec<String>,
    },
    Status {
        v: u32,
        token: PaneToken,
        generation: PaneGeneration,
        state: String,
        detail: Option<String>,
        deadline_epoch: Option<i64>,
        attempt: u32,
    },
    /// Ask for the pane this token names. Carries no generation: we are asking
    /// about a pane the host has never described to us, so we cannot know which
    /// spawn it is on — the `PaneOpened` the host answers with is what says.
    WatchPane { v: u32, token: PaneToken },
    /// The pane's program wants the person back. The host raises that pane's
    /// project tab marker; it carries no reason and no text.
    Attention {
        v: u32,
        token: PaneToken,
        generation: PaneGeneration,
    },
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

/// Parse one line the host wrote to our stdin.
///
/// Refuses an over-long line and a version we do not speak; both are conditions
/// this process cannot recover from by guessing.
pub fn decode_event(line: &str) -> anyhow::Result<PluginEvent> {
    anyhow::ensure!(
        line.len() <= MAX_LINE_BYTES,
        "host line is {} bytes, over the {MAX_LINE_BYTES}-byte limit",
        line.len()
    );
    let event: PluginEvent = serde_json::from_str(line)
        .map_err(|e| anyhow::anyhow!("cannot parse host event: {e}; unknown or malformed event"))?;
    let v = match &event {
        PluginEvent::PaneOpened { v, .. }
        | PluginEvent::PaneOutput { v, .. }
        | PluginEvent::PaneIdle { v, .. }
        | PluginEvent::PaneExited { v, .. }
        | PluginEvent::PaneClosed { v, .. }
        | PluginEvent::UserInput { v, .. }
        | PluginEvent::Shutdown { v } => *v,
    };
    anyhow::ensure!(
        v == PROTOCOL_VERSION,
        "host speaks protocol version {v}, this plugin speaks {PROTOCOL_VERSION}"
    );
    Ok(event)
}

/// Serialise one command as exactly one NDJSON line, without its terminator.
///
/// The framing is the newline, so an embedded one would split the frame;
/// `serde_json` escapes newlines inside strings, and this checks rather than
/// trusts that.
pub fn encode_command(cmd: &PluginCommand) -> anyhow::Result<String> {
    let line = serde_json::to_string(cmd)
        .map_err(|e| anyhow::anyhow!("cannot encode plugin command: {e}"))?;
    anyhow::ensure!(
        !line.contains('\n'),
        "encoded plugin command contains a newline, which would split the frame"
    );
    Ok(line)
}

pub fn watch_pane(token: PaneToken) -> PluginCommand {
    PluginCommand::WatchPane {
        v: PROTOCOL_VERSION,
        token,
    }
}

pub fn attention(token: PaneToken, generation: PaneGeneration) -> PluginCommand {
    PluginCommand::Attention {
        v: PROTOCOL_VERSION,
        token,
        generation,
    }
}

pub fn log(level: LogLevel, message: impl Into<String>) -> PluginCommand {
    PluginCommand::Log {
        v: PROTOCOL_VERSION,
        level,
        message: message.into(),
    }
}

#[cfg(test)]
#[path = "protocol_tests.rs"]
mod tests;
