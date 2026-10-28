# UI and Input

This document covers TUI project boundaries, key input routing, redraws, and the lower chrome contract. The TUI prioritizes programs running inside the terminal, so it minimizes intercepted input and layout changes.

## Keyboard routing

The default leader is `Ctrl+F` and can be changed to `ctrl+<letter>` under `[input] leader`. Pressing the leader interprets the next key as one app command and ends prefix state on every path: mapped, unmapped, `Esc`, or `Ctrl+C`. There is no timeout. `<leader> <leader>` sends a literal leader to the PTY when a terminal has focus.

- Leader commands are `t` (new pane), `w` (close pane), `s` (swap target), `z` (claim PTY size), `l` (log), `b` (tree), `f` (fullscreen), `o` (open project), `x` (close project), `p` (theme), `u` (reload config), `r` (refit PTY size and redraw), `q` (quit), and `c` (cancel recovery). Ordinary key press/repeat, paste, mouse down, and wheel claim sizing when the current TUI is interacted with. Window resize, focus, redraw, and reconnect do not change the owner. `c` appears in the hint only when recovery is pending. `w` and `s` run only when terminal focus and pane count meet their conditions.
- In split layout, leader digits `1`/`2` focus the file list/diff, while `3`–`9` and `0` select panes `0`–`7`. In terminal fullscreen, `1`–`8` map naturally to panes `0`–`7`; `9`/`0` are ignored. Bare `F1`–`F10` select project tabs `0`–`9` regardless of layout.
- Keys reserved without a prefix are bare F-keys and shift-only arrows/PageUp/PageDown. Other ordinary keys and standalone Ctrl keys go to the active backend's stdin. Thus the app does not steal editing keys such as `Ctrl+W` or `Ctrl+L` from a pane. Since the app uses bare F-keys, pane programs' F-key menus require a modifier.
- Paste is wrapped in escapes only when the terminal has bracketed-paste mode enabled; ESC/NUL bytes are removed. A paste arriving as a character burst from the Windows console is grouped into synthetic paste only within bounded timing/length limits. If detection is uncertain, the original key order is restored. An open overlay or missing project gives input ownership to the overlay/empty state first.
- Synthetic input such as scroll, mouse reports, and query replies is sent to the PTY only when the program requested the relevant mode. A terminal key that is not an app command does not count as input to dismiss a notice.

## Project boundary

`Workspace` manages up to `MAX_PROJECTS = 10` `App`s using a Vec and active index. Each `App` owns one repository's `GitViewManager`, pane set, focus/fullscreen/notice state. There may be no active project, so after the final tab closes only the repo dialog and quit remain available. Opening the same canonical worktree twice focuses its existing tab instead. Terminal attention in a hidden project is marked as read only for that TUI client.

The repo-open dialog is handled first at the Workspace level, so it works with zero projects. Path input does not launch a shell; it completes directory candidates by reading only one level with `read_dir`. `~` and relative paths are expanded only when read, while the text the user typed is preserved. The directory browser manages expansion/collapse as a flat row list, and `confirm_repo_input` on the field is the single path-confirmation point. Browser `Enter` moves the selection into the field and immediately confirms it in the same input event.

TUI workspace state is saved in `~/.nightcrow/workspace.json`. It records open projects, the active project, and each project's view, but is not stored inside a repository. Only values that need a snapshot, such as a restored status selection, remain pending; background project queues are drained each tick, but snapshots are applied only to the active project. See [session.md](session.md) for worker joining and snapshot watching details.

## Layout and redraw

`ui::chrome::chrome_areas` always creates four areas: project tabs, body, notice, and hint. Notice and hint occupy the bottom two rows (`bottom_rows`) regardless of layout. According to `[layout] tabs`, project tabs use either the first row above them (`top`) or the left `STRIP_WIDTH` (20) columns (`left`); the body uses the remaining area. The TUI layout config determines the body's upper/lower split, and terminal pane rectangles use the single geometry source described in [terminal.md](terminal.md). Notices and dialogs do not add or remove rows.

Input, PTY output, snapshot/load results, tree watch, resize, recovery, and title changes request a dirty frame. The event loop polls queues every 16 ms but does not call `Terminal::draw` on unchanged ticks. `<leader> r` explicitly requests ownership, resends current geometry, clears the front buffer, and performs a full repaint. Status hot-file fades and attention/search caret boundaries also dirty a frame on timer events. Opening repo input keeps the notice-row side borders fixed while showing two accent/dim pulses, then remains dim. This focus cue repaints only at phase boundaries and schedules no timer after it finishes.

## Notice row

Normally the notice row shows the repo display path, branch, tracking (`↑N ↓M`), and recovery marker, omitting a chip when its value is absent. If space is limited, only path and branch shrink; tracking/recovery widths are preserved. When `App::notice` is set it overlays the row without changing body size. Opening the repo dialog puts its input line in the notice row, and its notice and candidates/legend take priority in the hint row.

Notice expiry is determined by `NoticeKind`, not by message text. Only success of the same kind clears the notice, and only app-handled input dismisses it. PTY passthrough keys do not dismiss a notice, so an error does not disappear just because the user resumes typing.

← [Architecture index](../architecture.md)
