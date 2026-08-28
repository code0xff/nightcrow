# Session state

## Recent-activity focus indicator

Files modified within the last `hot_window_secs` seconds — whether by an agent in a terminal pane, your editor, or a build/format script — are rendered in the accent color (bold for the first 5 seconds, normal until the window expires).

When the file list is in focus and you have not navigated in the last 2 seconds, the selection auto-follows to the freshest hot file so the diff updates as files change. Manual navigation (`j` / `k` / arrows / PgUp / PgDn) immediately suppresses auto-follow until you go idle again.

Configurable under [`[agent_indicator]`](configuration.md#agent_indicator).

## What persists

nightcrow saves the current state on exit and restores it on the next launch — focus position, selected file, scroll offset, active terminal pane, view mode (status / commit log / tree), fullscreen states, commit-log drill-down position, and tree expansion and selection.

The accent is not in that list. It belongs to the session rather than to one repo's view state, so it lives in `~/.nightcrow/viewer.json` alongside the viewer's other shared preferences and is not restored per repo.

The browser keeps its own half of this. Which panel each project is maximized in is remembered per project in `viewer.json`, so a refresh comes back to the layout you left — the browser's counterpart to the fullscreen states above, kept apart from them because maximizing on a 40-row terminal and in a browser window are not the same answer. It is held for 50 projects, like the TUI's — the 50 whose arrangement was set most recently, so maximizing a fifty-first is what drops the oldest, not merely opening one.

Everything else lands in one file, `~/.nightcrow/workspace.json` — which repos were open, which tab was in front, and each repo's view state. Nothing is written inside your repositories: no single repo owns the fact that others were open beside it, and nightcrow shouldn't create directories in a project it is only reading.

A bare `nightcrow` reopens those tabs and lands on the one that was in front, with each project's selection and scroll where you left them. Repos that have moved or been deleted since are skipped, with a notice saying how many. View state is kept for the 50 most recently used repos.

## Who writes what

The two halves have two owners. The session writes which repositories are open and which tab is in front; an attached client writes what it had selected and scrolled, and never the tab list — detaching must not roll the session back to one client's view of it. To start empty, close every tab before stopping the session.
