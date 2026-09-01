# Keyboard and mouse

`<prefix>` means the configured leader key. It is `Ctrl+F` by default and can be changed with [`[input] leader`](configuration.md#session-and-client-settings). App commands use the leader followed by one key; ordinary keys, including bare `Ctrl` chords, go to the focused terminal.

The prefix waits indefinitely for one follow-up. `Esc` or `Ctrl+C` cancels it. An unmapped follow-up is consumed. Pressing the leader twice sends one literal leader chord to the focused terminal.

## Leader commands

- `<prefix> t` opens a terminal pane (up to 8 panes per project).
- `<prefix> w` closes the active terminal pane when the terminal has focus.
- `<prefix> s`, then a pane digit, swaps the active pane with the selected pane. `Esc` or `Ctrl+C` cancels the second step.
- `<prefix> z` claims the terminal size for this screen when another client currently owns it. A PTY has one size shared by all clients.
- `<prefix> c` cancels a plugin recovery pending for the focused pane.
- `<prefix> l` toggles between status and commit-log views.
- `<prefix> b` opens the read-only tree view.
- `<prefix> f` toggles fullscreen for the focused list, diff, or terminal panel. Terminal fullscreen cycles through the grid and the active-pane zoom.
- `<prefix> o` opens the repository dialog.
- `<prefix> x` closes the active project tab.
- `<prefix> p` cycles the session accent: yellow, cyan, green, magenta, blue.
- `<prefix> u` reloads the configuration; see [Reloading](configuration.md#reloading).
- `<prefix> r` forces a full redraw.
- `<prefix> q` detaches the TUI; it does not stop the session.
- `<prefix> 1` focuses the file list and `<prefix> 2` focuses the diff viewer in split view.
- `<prefix> 3`…`<prefix> 9` and `<prefix> 0` focus terminal panes 1–8 in split view (`0` is pane 8).
- In terminal fullscreen, `<prefix> 1`–`<prefix> 8` focus panes 1–8; `9` and `0` do nothing.

## Global keys

- `F1`–`F10` switch project tabs 1–10. Modified function keys pass through to the terminal.
- `Ctrl+Shift+Left` / `Ctrl+Shift+Right` switch to the previous or next project, wrapping at both ends of the tab order. With one project open they do nothing.
- `Shift+Left` / `Shift+Right` cycle focus through the file list, diff viewer, and terminal.
- `Shift+Up` / `Shift+Down` scroll the active terminal three lines.
- `Shift+PageUp` / `Shift+PageDown` scroll the active terminal one page. Input remains live while scrolled.

## File list and commit list

- `Up` / `Down` and `k` / `j` move the selection; `PageUp` / `PageDown` move by a page-sized step.
- `Left` / `Right` scroll long paths or commit summaries. In the tree they collapse or expand directories instead.
- `/` starts a search. In status it searches paths; in the log it searches commits or drilled-in files; in the tree it searches filenames.
- `Enter` confirms a search, drills into a selected commit, or opens a selected tree file.
- `Esc` clears a search. In the commit log, a second `Esc` leaves a drilled-down file list.

## Diff viewer

- `Up` / `Down`, `k` / `j`, `PageUp` / `PageDown`, and `Left` / `Right` scroll the diff. `/`, `n`, and `N` search and move between matches; `Esc` clears the search.
- `v` toggles a changed file's diff and whole-file content when both are available.
- `s` toggles unified and side-by-side diff layouts.
- `w` toggles soft wrapping in the unified/file view.
- `Tab` cycles unified diff, side-by-side diff, and whole-file content when a file is available.
- `Enter` or `<prefix> f` toggles diff fullscreen.

## Repository dialog

`<prefix> o` opens a path field. `Tab` completes a directory, `Down` opens the directory browser, and `Enter` opens the selected path. `Esc` closes the browser first and the dialog second. Paths may be absolute, relative to the current directory, or begin with `~`; shell expansion, variables, globs, and files are not accepted. See [Views → The repo dialog](views.md#the-repo-dialog).

## Mouse

Mouse capture is enabled by default. Click a project tab or panel to focus it; click a terminal pane to focus it and forward the report to programs that requested mouse input. The wheel scrolls the pane under the pointer. Clickable hint-bar commands behave like their key equivalents.

While capture is enabled, hold the outer terminal's selection modifier while dragging to select text: `Shift` in xterm-family terminals, `Option` in iTerm2, and `Fn` or `Option` in macOS Terminal.app. Set `[mouse] enabled = false` to restore ordinary outer-terminal selection and disable click forwarding.
