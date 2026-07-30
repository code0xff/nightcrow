use anyhow::{Result, anyhow};
use serde::{Deserialize, Serialize};

/// Env var carrying the pane token into the pane's child process.
///
/// A provider CLI spawns its own helper processes (hooks, statusline commands)
/// and those inherit this, which is what lets an out-of-process observer say
/// which pane an event came from. The working directory cannot answer that —
/// nightcrow deliberately allows several panes on one repository, so cwd
/// identifies the project rather than the pane.
pub const PANE_TOKEN_ENV: &str = "NIGHTCROW_PANE_TOKEN";

/// Entropy behind a pane token, matching the viewer's session tokens at half
/// the width: a session holds a handful of panes, not thousands.
const TOKEN_BYTES: usize = 16;

/// Opaque name for a pane slot, stable for as long as the slot exists.
///
/// [`PaneId`](super::PaneId) cannot serve this purpose outside the process that
/// owns the panes: it is a per-backend counter that restarts at 1 whenever a
/// backend is rebuilt, so the same number means different panes across two
/// runs. The token is random instead, and it deliberately outlives the process
/// occupying the slot — an observer tracking a slot keeps its state when the
/// slot's process is replaced.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct PaneToken(String);

impl PaneToken {
    /// Mint a token from OS entropy.
    ///
    /// Fallible rather than silently weakened: a predictable token would let
    /// anything that can guess one address a pane it was never given.
    pub fn new() -> Result<Self> {
        let mut bytes = [0u8; TOKEN_BYTES];
        getrandom::fill(&mut bytes)
            .map_err(|e| anyhow!("OS RNG unavailable for pane token: {e}"))?;
        let mut hex = String::with_capacity(TOKEN_BYTES * 2);
        for b in bytes {
            hex.push_str(&format!("{b:02x}"));
        }
        Ok(Self(hex))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Which spawn of a pane slot something refers to.
///
/// Starts at [`FIRST_GENERATION`] and rises every time the slot's process is
/// replaced. An out-of-process observer decides what to do asynchronously, so
/// by the time it asks for something the process it watched may already be
/// gone; carrying the generation is what makes that detectable instead of
/// letting a decision about one process land on its successor.
pub type PaneGeneration = u32;

pub const FIRST_GENERATION: PaneGeneration = 1;

/// A pane slot's identity: which slot, and which spawn within it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PaneIdentity {
    pub token: PaneToken,
    pub generation: PaneGeneration,
}

impl PaneIdentity {
    pub fn new() -> Result<Self> {
        Ok(Self {
            token: PaneToken::new()?,
            generation: FIRST_GENERATION,
        })
    }

    /// Advance to the next spawn of the same slot.
    ///
    /// Saturating rather than wrapping: a wrapped generation would make a stale
    /// command look current again, which is the one thing the counter exists to
    /// prevent. A slot that somehow reached `u32::MAX` relaunches stops being
    /// able to distinguish spawns, and refusing to move is the safe end state.
    pub fn advance(&mut self) {
        self.generation = self.generation.saturating_add(1);
    }
}

#[cfg(test)]
#[path = "identity_tests.rs"]
mod tests;
