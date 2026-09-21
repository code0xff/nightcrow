//! What the help overlay says.
//!
//! One table for the whole TUI: the overlay renders it and nothing else, so a
//! command gains a help row by being listed here. `{L}` stands in for the
//! configured leader so a rebound leader is printed, not the default.

pub(crate) struct HelpSection {
    pub title: &'static str,
    pub rows: &'static [HelpRow],
}

pub(crate) struct HelpRow {
    pub keys: &'static str,
    pub what: &'static str,
}

pub(crate) const HELP_SECTIONS: &[HelpSection] = &[
    HelpSection {
        title: "The leader",
        rows: &[
            HelpRow {
                keys: "{L}",
                what: "Arm the leader; the next key runs one command. Waits indefinitely.",
            },
            HelpRow {
                keys: "{L} {L}",
                what: "Send one literal leader chord to the focused terminal pane.",
            },
            HelpRow {
                keys: "esc / ctrl+c",
                what: "Cancel an armed leader. An unmapped follow-up is consumed.",
            },
        ],
    },
    HelpSection {
        title: "Terminal panes",
        rows: &[
            HelpRow {
                keys: "{L} t",
                what: "Open a terminal pane, up to 8 per project.",
            },
            HelpRow {
                keys: "{L} w",
                what: "Close the active pane, while the terminal has focus.",
            },
            HelpRow {
                keys: "{L} s, digit",
                what: "Swap the active pane with the pane that digit names.",
            },
            HelpRow {
                keys: "{L} z",
                what: "Claim the shared PTY size for this screen.",
            },
            HelpRow {
                keys: "{L} c",
                what: "Cancel a plugin recovery pending for the focused pane.",
            },
            HelpRow {
                keys: "shift+up/down",
                what: "Scroll the active pane three lines.",
            },
            HelpRow {
                keys: "shift+pgup/pgdn",
                what: "Scroll the active pane one page. Input stays live.",
            },
        ],
    },
    HelpSection {
        title: "Views and focus",
        rows: &[
            HelpRow {
                keys: "{L} l",
                what: "Toggle between the status view and the commit log.",
            },
            HelpRow {
                keys: "{L} b",
                what: "Open the read-only tree view.",
            },
            HelpRow {
                keys: "{L} f",
                what: "Fullscreen the focused panel; in the terminal, cycle grid and zoom.",
            },
            HelpRow {
                keys: "{L} 1 / {L} 2",
                what: "Focus the file list / the diff viewer, in split view.",
            },
            HelpRow {
                keys: "{L} 3-9, {L} 0",
                what: "Focus terminal panes 1-8. In terminal fullscreen, {L} 1-8 do.",
            },
            HelpRow {
                keys: "shift+left/right",
                what: "Cycle focus through list, diff, and each pane in turn.",
            },
        ],
    },
    HelpSection {
        title: "Projects",
        rows: &[
            HelpRow {
                keys: "{L} o",
                what: "Open the repository dialog. Tab completes, Down browses.",
            },
            HelpRow {
                keys: "{L} x",
                what: "Close the active project tab.",
            },
            HelpRow {
                keys: "{L} [ / {L} ]",
                what: "Move the active tab one slot forward / back. Neither wraps.",
            },
            HelpRow {
                keys: "f1-f10",
                what: "Switch to project tabs 1-10.",
            },
            HelpRow {
                keys: "ctrl+shift+left/right",
                what: "Switch to the previous / next project, wrapping.",
            },
        ],
    },
    HelpSection {
        title: "Lists and the diff",
        rows: &[
            HelpRow {
                keys: "up/down, k/j",
                what: "Move the selection or scroll; PgUp/PgDn move a page.",
            },
            HelpRow {
                keys: "left/right",
                what: "Scroll long lines; in the tree, collapse and expand.",
            },
            HelpRow {
                keys: "/ then enter",
                what: "Search the view; n and N step matches in the diff.",
            },
            HelpRow {
                keys: "enter",
                what: "Drill into a commit, open a tree file, or toggle diff fullscreen.",
            },
            HelpRow {
                keys: "esc",
                what: "Clear a search; a second esc leaves a drilled-down file list.",
            },
            HelpRow {
                keys: "v / s / w / tab",
                what: "Whole file, side-by-side, soft wrap, and cycling between them.",
            },
        ],
    },
    HelpSection {
        title: "Session",
        rows: &[
            HelpRow {
                keys: "{L} p",
                what: "Cycle the session accent: yellow, cyan, green, magenta, blue.",
            },
            HelpRow {
                keys: "{L} u",
                what: "Reload the configuration file.",
            },
            HelpRow {
                keys: "{L} r",
                what: "Force a full redraw.",
            },
            HelpRow {
                keys: "{L} q",
                what: "Detach the TUI. The session and its panes keep running.",
            },
        ],
    },
    HelpSection {
        title: "Mouse",
        rows: &[
            HelpRow {
                keys: "click",
                what: "Focus a tab, a panel, or a pane; hint-bar commands are clickable.",
            },
            HelpRow {
                keys: "click a link",
                what: "Open http(s) in the browser, a local path in a GUI editor.",
            },
            HelpRow {
                keys: "drag with modifier",
                what: "Select text: shift, or option in iTerm2 and Terminal.app.",
            },
        ],
    },
];
