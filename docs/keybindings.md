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
- `<prefix> [` and `<prefix> ]` move the active project one slot towards the front or the back of the tab row. Tab order is shared with the browser and every other attached TUI. Neither wraps: the first tab does not move further forward and the last does not move further back.
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

## Web viewer

The browser binds the same commands, not the same physical keys. The keys above are free for the TUI to take because the terminal hands them over; a browser has already spent most of them, so the web viewer names user actions and binds each to whatever key it can actually receive. The tables below say which of the commands above survive unchanged, which keep the intent through a different mechanism, and which have no browser answer at all.

Its leader is also `Ctrl+F` by default, and it is consumed only where the page owns the keyboard — the app chrome and the terminal panel. In a text field, a dialog, or during IME composition the leader is never intercepted, so typing and native word selection are untouched. Pressing the leader twice sends one literal leader chord to the focused pane, as in the TUI. `Esc` or `Ctrl+C` cancels an armed leader, and it also clears itself on focus loss, on a dialog opening, on a project switch, and on a terminal reconnect, so the following key is never swallowed.

`Ctrl+F` is the browser's Find shortcut, so the viewer says so and lets the leader be rebound or switched off. Holding a shortcut down runs it once per press, not once per repeat.

### Same meaning as the TUI

The leader followed by `t`, `w`, `s`, `z`, `c`, `l`, `b`, `o`, `x`, `p`, or `u` does what the matching bullet under [Leader commands](#leader-commands) describes, using the same controls the buttons use. The focus keys `1`, `2`, and `3`–`9`, `0` address the list, the content pane, and terminal panes 1–8 with the same numbering. `Ctrl+Shift+Left` and `Ctrl+Shift+Right` switch projects exactly as they do in the TUI.

### Reinterpreted

- Moving a project within the tab row is a drag on the tab itself rather than a key. The order it writes is the same session-owned order the TUI's bracket keys move.
- The leader followed by `f` maximizes the focused panel and zooms the active terminal pane. A page cannot take the browser's chrome into fullscreen, and `F11` belongs to the browser, so the intent — give this panel the whole area — is kept and the mechanism is not.

### Not bound in the browser

- Redraw: the browser repaints the page itself, so there is no stale frame to force.
- Detach: closing a tab already leaves the session running. Signing out is a different, destructive action and is deliberately not on a key.
- `F1`–`F10` project selection: bare function keys are reserved by the browser and the OS. Use `Ctrl+Shift+Left` / `Ctrl+Shift+Right`, the project control, or the shortcut sheet instead.

`F5` and `F11` are never bound, and the viewer does not try to block them. Which chords a browser delivers to a page at all is the browser's decision, not the viewer's: the bindings above avoid the ones Chrome, Edge, Firefox, and Safari reserve on Windows, macOS, and Linux, and a chord a browser keeps for itself simply never arrives. The shortcut sheet lists every action with its key and marks the ones unavailable on the current screen, and every action also has a button or menu item. The one exception is `<prefix> s`, which arms a second step rather than running a command: no single control can stand for "then pick a pane", so the sheet lists it as text and dragging a pane does the same job.

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
