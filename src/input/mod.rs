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
    /// Arm swap mode: the next digit picks the pane to swap the active pane
    /// with. Emitted by the `<leader> s` follow-up; the digit is resolved in a
    /// separate tick (see `handle_swap_target_followup` in `main`).
    SwapPanePrompt,
    /// Focus the project tab at this index. Out-of-range indices are inert.
    SwitchProject(usize),
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
    Redraw,
    None,
}

mod encode;
mod routing;

pub use encode::{encode_arrow, encode_button, encode_key, encode_wheel, encode_wheel_horizontal};
pub use routing::{map_key, prefix_action, prefix_action_fullscreen, vim_navigation_action};

#[cfg(test)]
mod tests;