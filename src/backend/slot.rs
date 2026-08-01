use super::PaneId;
use super::identity::{PaneIdentity, PaneToken};
use anyhow::{Result, bail};
use std::collections::BTreeMap;
use std::time::{Duration, Instant};

/// What it takes to put a pane's process back after it exits.
///
/// The hub discards the startup command once a pane is spawned. Keeping the text
/// here is what makes a relaunch reproduce the original launch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PaneLaunch {
    /// Startup command exactly as configured, or `None` for a bare shell.
    pub command: Option<String>,
}

/// A pane slot: who it is, what it was launched as, and when it last spoke.
#[derive(Debug)]
pub struct PaneSlot {
    pub identity: PaneIdentity,
    pub launch: PaneLaunch,
    last_output: Instant,
}

impl PaneSlot {
    /// How long the pane has been quiet.
    ///
    /// Measured from the last byte the child produced, not from the last thing
    /// written to it.
    pub fn idle_for(&self, now: Instant) -> Duration {
        now.saturating_duration_since(self.last_output)
    }
}

/// Per-pane slot bookkeeping, held beside the live PTYs.
///
/// Separate from the PTY map because the two have different lifetimes: a
/// relaunch replaces the PTY while the slot has to survive it.
#[derive(Debug, Default)]
pub struct PaneSlots(BTreeMap<PaneId, PaneSlot>);

impl PaneSlots {
    pub fn insert(&mut self, id: PaneId, identity: PaneIdentity, launch: PaneLaunch, now: Instant) {
        self.0.insert(
            id,
            PaneSlot {
                identity,
                launch,
                // A pane that has said nothing yet counts as quiet since it
                // opened, so a plugin does not have to wait for first output
                // before a freshly opened pane can be considered idle.
                last_output: now,
            },
        );
    }

    pub fn remove(&mut self, id: PaneId) -> Option<PaneSlot> {
        self.0.remove(&id)
    }

    pub fn get(&self, id: PaneId) -> Option<&PaneSlot> {
        self.0.get(&id)
    }

    /// Note that the pane produced output.
    pub fn mark_output(&mut self, id: PaneId, now: Instant) {
        if let Some(slot) = self.0.get_mut(&id) {
            slot.last_output = now;
        }
    }

    /// Find the pane a token names, if it still exists.
    ///
    /// Linear over a handful of panes; a reverse index would be more state to
    /// keep consistent than the scan costs.
    pub fn find_by_token(&self, token: &PaneToken) -> Option<PaneId> {
        self.0
            .iter()
            .find(|(_, slot)| &slot.identity.token == token)
            .map(|(id, _)| *id)
    }
}

/// Longest resume argument list a plugin may append.
///
/// A resume invocation is a flag and an id; anything longer is not a resume.
const MAX_RESUME_ARGS: usize = 6;

/// Longest single resume argument. Comfortably past a UUID or a session name.
const MAX_RESUME_ARG_LEN: usize = 256;

/// Characters a resume argument may consist of.
///
/// Deliberately narrower than "anything the shell can be made to swallow": the
/// argument is appended to a command line that a login shell parses, so a value
/// carrying a space, quote, backtick, `$`, or `;` is refused outright rather
/// than escaped differently by every supported shell.
fn is_safe_arg_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-' | ':' | '/' | '=' | '@' | '+')
}

/// Build the command line for a relaunch.
///
/// `allowed_flags` is the plugin's declared list from config. The first token
/// (flag or subcommand) and every option-like token must appear there. Values
/// following an approved control token remain provider data such as a session
/// id. This lets the core refuse an unapproved relaunch mode without knowing a
/// particular CLI's grammar.
pub fn resume_command_line(
    base: Option<&str>,
    resume_args: &[String],
    allowed_flags: &[String],
) -> Result<String> {
    let Some(base) = base else {
        // Nothing was configured to run, so there is no session to resume and
        // no original invocation to preserve.
        bail!("pane has no startup command to relaunch");
    };
    if resume_args.is_empty() {
        return Ok(base.to_string());
    }
    if resume_args.len() > MAX_RESUME_ARGS {
        bail!(
            "relaunch passed {} arguments, at most {MAX_RESUME_ARGS} allowed",
            resume_args.len()
        );
    }

    let mut line = String::from(base);
    for (index, arg) in resume_args.iter().enumerate() {
        if arg.is_empty() {
            bail!("relaunch argument must not be empty");
        }
        if arg.len() > MAX_RESUME_ARG_LEN {
            bail!(
                "relaunch argument is {} bytes, at most {MAX_RESUME_ARG_LEN} allowed",
                arg.len()
            );
        }
        if !arg.chars().all(is_safe_arg_char) {
            bail!("relaunch argument {arg:?} holds characters that are not allowed");
        }
        let requires_approval = index == 0 || arg.starts_with(['-', '/']);
        if requires_approval && !allowed_flags.iter().any(|allowed| allowed == arg) {
            bail!(
                "relaunch token {arg:?} is not in the plugin's allowed_resume_flags; \
                 add it there if the plugin is meant to pass it"
            );
        }
        line.push(' ');
        // Every permitted character is literal in both POSIX shells and
        // `cmd.exe`. Adding POSIX single quotes here would make those quote
        // bytes part of the argument on Windows.
        line.push_str(arg);
    }
    Ok(line)
}

#[cfg(test)]
#[path = "slot_tests.rs"]
mod tests;
