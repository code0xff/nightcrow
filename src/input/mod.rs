#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Quit,
    Up,
    Down,
    PageUp,
    PageDown,
    NewPane,
    ClosePane,
    ToggleFullscreen,
    SwitchPane(usize),
    /// Take over sizing this project's panes.
    ///
    /// In a shared session one client's layout sets the PTY sizes and the rest
    /// render that grid; this asks for it. Inert when this client already has
    /// it, and when its panes are its own.
    ClaimPaneSizing,
    /// Give up on the recovery a plugin has pending for a pane.
    ///
    /// Inert when nothing is pending. Behind the leader like every other app
    /// command: a bare key in a terminal pane belongs to the program in it.
    CancelRecovery,
    /// Arm swap mode: the next digit picks the pane to swap the active pane
    /// with. Emitted by the `<leader> s` follow-up; the digit is resolved in a
    /// separate tick (see `handle_swap_target_followup` in `main`).
    SwapPanePrompt,
    /// Focus the project tab at this index. Out-of-range indices are inert.
    SwitchProject(usize),
    /// Step one project tab towards the front of the list, wrapping. The
    /// relative counterpart to the F-key jumps, for a session with more tabs
    /// than the user wants to count.
    PrevProject,
    /// Step one project tab away from the front of the list, wrapping.
    NextProject,
    /// Open the repo-path dialog to add a project tab.
    OpenProject,
    /// Close the active project tab. Refused when it is the only one.
    CloseProject,
    FocusList,
    FocusDiff,
    CycleForward,
    CycleBackward,
    TermScrollUp,
    TermScrollDown,
    TermScrollLineUp,
    TermScrollLineDown,
    ToggleLogView,
    ToggleTreeView,
    CycleTheme,
    /// Ask the session to re-read `config.toml`. A session-wide request rather
    /// than a local one — the daemon owns the plugins and the startup list.
    ReloadConfig,
    Redraw,
    None,
}

mod encode;
mod routing;

pub use encode::{encode_arrow, encode_button, encode_key, encode_wheel, encode_wheel_horizontal};
pub use routing::{map_key, prefix_action, prefix_action_fullscreen, vim_navigation_action};

#[cfg(test)]
mod tests;
