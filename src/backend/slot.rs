use super::PaneId;
use super::identity::{PaneIdentity, PaneToken};
use anyhow::{Result, bail};
use std::collections::BTreeMap;
use std::time::{Duration, Instant};

/// What it takes to put a pane's process back after it exits.
///
/// The hub discards the startup command once a pane is spawned, which is fine
/// until something wants to replace the process rather than the pane: a shell
/// cannot be asked what it was told to run. Keeping the text here is what makes
/// a relaunch reproduce the original launch instead of guessing at it.
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
    /// written to it: a CLI that is mid-answer keeps emitting, and typing into
    /// it then would interleave with what it is drawing.
    pub fn idle_for(&self, now: Instant) -> Duration {
        now.saturating_duration_since(self.last_output)
    }
}

/// Per-pane slot bookkeeping, held beside the live PTYs.
///
/// Separate from the PTY map because the two have different lifetimes: a
/// relaunch replaces the PTY while the slot — and so the token an outside
/// observer holds — has to survive it.
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
/// than trusted to survive quoting. Quoting is applied as well — this is the
/// belt to that braces.
fn is_safe_arg_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-' | ':' | '/' | '=' | '@' | '+')
}

/// Build the command line for a relaunch.
///
/// `allowed_flags` is the plugin's declared list from config. Anything that
/// looks like a flag must appear there, which is how the core refuses to weaken
/// a CLI's permission posture without knowing what any particular CLI's
/// permission flags are called — the user names what a plugin may pass, and a
/// flag they did not name cannot be smuggled in.
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
    for arg in resume_args {
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
        if arg.starts_with('-') && !allowed_flags.iter().any(|f| f == arg) {
            bail!(
                "relaunch flag {arg:?} is not in the plugin's allowed_resume_flags; \
                 add it there if the plugin is meant to pass it"
            );
        }
        line.push(' ');
        line.push_str(&shell_quote(arg));
    }
    Ok(line)
}

/// Wrap a value so a login shell reads it as one literal word.
///
/// Single quotes suspend every expansion the shell would otherwise perform, so
/// the only character needing care is the quote itself.
fn shell_quote(arg: &str) -> String {
    let mut out = String::with_capacity(arg.len() + 2);
    out.push('\'');
    for c in arg.chars() {
        if c == '\'' {
            out.push_str("'\\''");
        } else {
            out.push(c);
        }
    }
    out.push('\'');
    out
}

#[cfg(test)]
#[path = "slot_tests.rs"]
mod tests;
