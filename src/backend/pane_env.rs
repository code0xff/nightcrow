//! The terminal-capability environment every pane's child is born with.
//!
//! A pane is rendered by a truecolor-capable emulator whichever way it is
//! viewed, so its child is told that rather than left to inherit whatever the
//! daemon's own parent was. The daemon is often started from somewhere that is
//! not a terminal at all — an agent's shell, a service manager — and such a
//! parent commonly exports `NO_COLOR=1` and `TERM=dumb` for its own children.
//! Forcing `TERM` alone was not enough: `NO_COLOR` carried through, and every
//! colour-aware program in the pane (Claude Code, Codex, `ls`) obeyed it and
//! drew in plain white.

use portable_pty::CommandBuilder;

/// What the pane's emulators — the hub's, the TUI's, xterm.js in the browser —
/// actually implement.
pub(super) const TERM: &str = "xterm-256color";

/// Truecolor is honoured end to end, and programs that gate 24-bit output on
/// this variable would otherwise fall back to the 256-colour palette.
pub(super) const COLORTERM: &str = "truecolor";

/// The one standard switch that turns colour off outright (<https://no-color.org>).
/// Removed rather than set empty: the convention is "present, non-empty", but
/// not every implementation checks for non-empty.
pub(super) const NO_COLOR: &str = "NO_COLOR";

/// Describe the pane's terminal to `cmd`, overriding whatever the daemon
/// inherited.
pub(super) fn apply(cmd: &mut CommandBuilder) {
    cmd.env("TERM", TERM);
    cmd.env("COLORTERM", COLORTERM);
    cmd.env_remove(NO_COLOR);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::OsStr;

    fn builder_with(vars: &[(&str, &str)]) -> CommandBuilder {
        let mut cmd = CommandBuilder::new("sh");
        for (key, value) in vars {
            cmd.env(key, value);
        }
        cmd
    }

    #[test]
    fn a_child_is_told_the_terminal_it_actually_runs_in() {
        let mut cmd = builder_with(&[("TERM", "dumb"), ("COLORTERM", "")]);
        apply(&mut cmd);
        assert_eq!(cmd.get_env("TERM"), Some(OsStr::new(TERM)));
        assert_eq!(cmd.get_env("COLORTERM"), Some(OsStr::new(COLORTERM)));
    }

    #[test]
    fn an_inherited_no_color_does_not_reach_the_child() {
        let mut cmd = builder_with(&[("NO_COLOR", "1")]);
        apply(&mut cmd);
        assert_eq!(cmd.get_env("NO_COLOR"), None);
    }

    #[test]
    fn other_inherited_variables_are_left_alone() {
        let mut cmd = builder_with(&[("FORCE_COLOR", "1"), ("EDITOR", "vim")]);
        apply(&mut cmd);
        assert_eq!(cmd.get_env("FORCE_COLOR"), Some(OsStr::new("1")));
        assert_eq!(cmd.get_env("EDITOR"), Some(OsStr::new("vim")));
    }
}
