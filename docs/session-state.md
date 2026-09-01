# Session state

## Recent activity

When `[agent_indicator].enabled` is true, files changed within `hot_window_secs` (15 seconds by default) are highlighted in the status list. They are bold for the first 5 seconds, then use the accent color until the window expires. This includes changes made by editors, builds, and terminal programs, not only AI tools.

With `[agent_indicator].auto_follow = true`, the status selection moves to the freshest hot file after 2 seconds without manual navigation. Moving the selection suppresses auto-follow until the next idle period. The indicator is shared by the TUI and browser and uses the server's setting.

## Files and ownership

State is stored under `~/.nightcrow/`; nightcrow does not write session state into a repository.

- `workspace.json` stores the daemon's open repositories, tab order, and active tab. It also stores up to 50 recently used repositories' TUI view state: selected file, focus, scroll, active pane, view mode, commit-log position, tree selection/expansion, and list/diff/terminal fullscreen state. A terminal fullscreen restore returns to the grid; a zoomed pane is not persisted.
- `viewer.json` stores the session accent and browser layout preferences, including sidebar width, upper-panel split, per-project view, and maximized panel (up to 50 recent projects). Browser terminal panes and their live arrangement end with the session.
- `sessions` stores authenticated web-viewer tokens so browser logins can survive a daemon restart. Logout revokes a token server-side. Removing this file prevents tokens from being restored on the next restart; a running daemon keeps its in-memory tokens until they expire or are logged out.

The daemon owns the repository set, the tab order, and the active tab. An attached TUI writes its own selection and view state when it detaches or the connection ends, without overwriting the tab list. Browser repository changes update the shared workspace. Closing every project before stopping writes an empty set, so the next session starts empty.

Corrupt or missing JSON state falls back to defaults. A repository that is no longer a directory is not started on the next daemon launch. Reopen it through the project dialog when it is available again.
