# Keyboard and mouse

## The leader key

nightcrow uses a tmux-style **leader (prefix)** key for its app commands. The default leader is `Ctrl+F` (configurable via `[input] leader`). `Ctrl+F` is a one-handed left-hand chord that avoids tmux's own `Ctrl+B` prefix (so nightcrow stays usable inside a tmux session), terminal flow control (`Ctrl+Q`/`Ctrl+S`), the shell signals (`Ctrl+C/D/Z`), and the Ctrl chords an inner Claude Code pane reserves (`Ctrl+G` is its external editor, plus `Ctrl+O/R/S/T/L`) — its only claimant is `Ctrl+F` as forward-char/page-forward, which most users reach via the arrow keys instead.

Press the leader, then a single follow-up key. Every other key — including Ctrl chords like `Ctrl+W` and `Ctrl+L` — passes straight through to the focused terminal, so a CLI running there (claude, codex, your shell) receives them unchanged. This is why the leader exists: cockpit users live inside the terminal panes and need their prompt-editing keys to reach the program, not nightcrow.

The hint bar shows the active leader in caret notation at its left edge (e.g. `^F: leader` for the default `Ctrl+F`), so the configured prefix is always visible from the terminal pane.

> **Migration from earlier versions:** the old bare-`Ctrl` app shortcuts moved behind the leader. `Ctrl+T/W/L/O/P/Q` are now `<prefix> t/w/l/o/p/q` and pass through to the terminal program instead; `Ctrl+F` is now the leader itself (`<prefix> f` toggles fullscreen). The old `Ctrl+Q`-twice quit confirmation is gone; leave with `<prefix> q`, which now detaches rather than ending the session — stop the session itself with `nightcrow stop`.

## Leader commands

Press `<prefix>`, then the key.

| Key | Action |
|-----|--------|
| `<prefix>` then `<prefix>` | Send the literal leader to the terminal program |
| `<prefix> t` | Open new terminal pane |
| `<prefix> w` | Close active terminal pane — terminal focus only, since without it no pane is highlighted as the close target |
| `<prefix> s` then `3`…`9`,`0` | Swap the active terminal pane with pane 1…8 (focus follows the pane; same pane numbering as the jump keys, so in terminal fullscreen the swap digits are `1`…`8`) — terminal focus only, like `w`, and needs at least two panes |
| `<prefix> z` | Resize the session's terminal panes to fit this screen. A PTY has one size and a program drawing on an alternate screen cannot be re-flowed afterwards, so one screen decides it for the whole session — whichever viewer opened most recently, until another asks. While someone else holds it (a second terminal, or a browser tab) this one renders that grid: padded if it is smaller than the pane, cropped if larger. Advertised in the hint bar only while that is the case |
| `<prefix> c` | Give up on the recovery a plugin has pending for a pane — the held slot is released, so nothing can be relaunched into it, and every attached client is told. Targets the focused pane's recovery, or the pane whose process has already ended while its slot was being held (that pane has no tab to focus). Advertised in the hint bar only while something is actually pending |
| `<prefix> l` | Toggle between status view and commit log view |
| `<prefix> b` | Toggle the read-only file-tree view (returns to status view) |
| `<prefix> f` | Fullscreen the focused pane. For the terminal it cycles `off → grid (all panes) → zoom (active pane only) → off`; with a single pane it toggles straight off/on. File list and diff viewer toggle off/on |
| `<prefix> o` | Open a repo in a **project tab** (prefilled with the active project's path — type to replace it, or press `→`/`End` first to extend it). `Tab` completes the path against your filesystem and `↓` opens a directory browser (see [Views](views.md#the-repo-dialog)). A leading `~` expands to your home directory. If another tab already has that repo open, nightcrow focuses that tab instead of running two copies against one worktree |
| `<prefix> x` | Close the active project tab. Closing the last one leaves nightcrow with no project open, which is a normal state |
| `<prefix> p` | Cycle accent color (yellow → cyan → green → magenta → blue). The accent belongs to the session, so every attached TUI and every open browser follows |
| `<prefix> u` | Re-read `config.toml` without restarting the session. `[[plugin]]` is re-applied to every open project immediately; `[[startup_command]]` applies to projects you open afterwards, because the panes an open project already started are live processes. Everything else in the file still needs a restart. The result appears on the notice row — see [Reloading the config](configuration.md#reloading-the-config) |
| `<prefix> r` | Force a full redraw (clears stray glyphs left by terminal programs) |
| `<prefix> q` | Detach — the TUI leaves and the session keeps running, terminals and all. Reattach with `nightcrow attach`; end the session itself with `nightcrow stop` |
| `<prefix> 1` / `<prefix> 2` | Focus the file/commit list / diff viewer — **split view only** |
| `<prefix> 3`…`<prefix> 9`, `<prefix> 0` | Jump to terminal pane 1…8 (`0` addresses pane 8) |
| `<prefix> 1`…`<prefix> 8` (terminal fullscreen) | Jump to terminal pane 1…8. With the viewer hidden the digit row addresses panes by natural numbering; `9`/`0` are unused. The only way back to the list/diff is `<prefix> f` to leave fullscreen |
| `Esc` / `Ctrl+C` (while armed) | Cancel the prefix |

The prefix has no timeout: once armed it waits indefinitely for the follow-up key. A key with no leader binding cancels the prefix and is dropped.

`<prefix> s` is the one two-step chord: it arms a swap mode (shown as `SWAP` in the hint bar) that waits for a pane digit, then swaps the active pane with the chosen one. A non-digit follow-up or `Esc` cancels swap mode without reordering.

## Global (no prefix)

| Key | Action |
|-----|--------|
| `Shift+→` / `Shift+←` | Cycle focus: file list → diff viewer → terminal panes → … |
| `F1`…`F10` | Switch to project tab 1…10 — see [Projects](projects.md). Unlike the pane digits, this mapping does not change with the layout: the same F-key reaches the same project in every view, fullscreen included |

A modified F-key (`Ctrl+F1`, `Shift+F5`, …) is not intercepted and passes through to the terminal program.

## File list / commit list (left panel)

| Key | Action |
|-----|--------|
| `↑` / `k`, `↓` / `j` | Navigate items one by one |
| `PgUp` / `PgDn` | Jump 10 items |
| `←` / `→` | Scroll long paths and commit summaries horizontally (in tree view these expand/collapse instead) |
| `<prefix> f` | Zoom the list pane to full screen (toggle) |
| `/` | Incremental search (status: paths; log: commit summaries; drill-down: paths; tree: recursive filenames) |
| `Esc` | Clear filter, then exit drill-down (log), then cancel search bar |
| `Enter` | Confirm filter (keeps query), drill into commit's file list (log view), or open the selected file fullscreen (tree view) |

## Diff viewer (right panel)

| Key | Action |
|-----|--------|
| `↑` / `k`, `↓` / `j` | Scroll one line |
| `PgUp` / `PgDn` | Scroll 20 lines |
| `←` / `→` | Horizontal scroll (4 columns) |
| `v` | Toggle between hunk diff and full file preview |
| `w` | Toggle soft wrapping of long lines. On, the tail of a long line continues on the next row instead of needing `←`/`→`; the line number folds into the line rather than sitting in its own column, so a continuation row carries no number. Horizontal scrolling is inert while wrapping (and the offset resets when you turn it on). The split view ignores wrapping — halves folding to different heights would stop lining up |
| `Tab` | Cycle the display: unified diff → side-by-side split → file contents → unified. `v` and `s` each toggle one view against the unified default, so the third stays hidden unless you know it exists; `Tab` walks all three. Skips the file step when there is no file to open, and does nothing in tree view |
| `s` | Toggle between the unified diff and a side-by-side split view (falls back to unified when the pane is too narrow) |
| `<prefix> f` | Zoom the diff/file pane to full screen (toggle) |
| `Enter` | Zoom the diff/file pane to full screen (toggle) — same as `<prefix> f` |
| `/` | Open search (works in both diff and file preview, including tree mode) |
| `n` / `N` | Next / previous search match |
| `Esc` | Clear search |

**Line numbers** are always shown in a pinned gutter. The unified view shows both sides (old, new) — an added line leaves the old column blank, a removed line leaves the new one blank. The split view numbers each half with the side it shows, and the file view (`v`) numbers the file itself. The gutter stays in place while `←`/`→` scroll the code.

## Terminal panes (bottom)

Every visible pane renders at once as a split grid instead of switching between tabs — 2 panes go side by side (or stacked if the terminal is narrow), 4 form a 2x2 grid, up to 4 show normally and up to 8 in the fullscreen grid. `<prefix> f` cycles the terminal through `off → grid → zoom → off`: *grid* hides the top viewer and fills the screen with the split grid, *zoom* fills the screen with just the active pane.

The active pane's cell is bordered in the accent color; jumping focus with `<prefix> 3`–`9`,`0` or `Shift+←/→` moves that border (and, while zoomed, the pane on screen) without closing any other pane. With more panes than fit, the tab bar shows a `+N` marker for the ones scrolled out of view — they keep running in the background. Keyboard input, paste, and scroll still target only the active pane. A single pane draws with no cell border.

| Key | Action |
|-----|--------|
| `Shift+↑` / `Shift+↓` | Scroll terminal output 3 lines |
| `Shift+PgUp` / `Shift+PgDn` | Scroll terminal output one page |

While scrolled, the terminal border title shows `[SCROLL — shift+pgdn: down | input: live]`. Keyboard input is still forwarded to the running process; `Shift+PgDn` to scroll back to the bottom.

The tab bar picks up OSC 0/2 window-title escape sequences, so programs like `claude`, `vim`, `ssh`, or `cd`-aware shell prompts can rename their own tab. Panes without an emitted title fall back to a default label.

## Mouse

nightcrow captures the mouse by default (`[mouse]` in the [configuration](configuration.md#mouse)):

- **Click a pane** to focus it, same as a jump key. The click is also forwarded to programs that asked for mouse reports (Claude Code, `less --mouse`, …) — so their clickable UI, like Claude Code's jump-to-bottom control, works. A plain shell receives nothing.
- **Click the file list or diff viewer** to focus that panel, same as `<prefix> 1`/`2`.
- **Click a project tab** in the top row to switch to it, same as its `F`-key. A `+N` overflow marker jumps to the nearest project folded behind it.
- **Wheel** scrolls the pane under the pointer, routed exactly like the scroll keys (wheel reports, arrow keys, or scrollback — whatever the program expects).
- **Click a tab** in the terminal tab bar to jump to that pane; clicking a `+N` hidden-pane marker reveals the nearest hidden pane on that side.
- **Click `o: open project`** on the empty screen — with no project open it is the one action the hint bar offers, and it dispatches like its key.
- **Click a shortcut** in the bottom hint bar to run it — command hints like `t: new pane`, `w: close pane`, or `f: fullscreen` dispatch exactly as if you pressed the keys they name. Clickable hints render inverted (reverse video) across their whole label so they stand out from informational hints; the inversion disappears when `[mouse]` is disabled. Navigation hints and `q: detach` are not clickable (detaching stays a deliberate two-key act).
- **Select text with a bypass modifier + drag.** While the mouse is captured, the outer terminal performs its native selection and copy only when you hold its bypass modifier while dragging. The modifier depends on the terminal: **Shift** in xterm-family terminals (Alacritty, kitty, GNOME Terminal, Windows Terminal), **Option (⌥)** in iTerm2, **Fn or Option** in macOS Terminal.app. Set `enabled = false` under `[mouse]` to give the mouse back to the outer terminal entirely — plain-drag selection returns, click forwarding stops.
