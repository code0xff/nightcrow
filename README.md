# nightcrow

Agent-adjacent terminal workbench — git diff viewer, commit log, and multi-pane terminal multiplexer in one window. Tuned for sitting next to LLM CLIs (Claude Code, Codex, aider) or any process that touches your working tree, but nightcrow itself has no AI ontology — it watches files and PTYs, not agents.

nightcrow runs as a **session**: one process holds the repositories and the terminals, and you reach it from a terminal (`nightcrow attach`) or a browser. Closing a client leaves the session running.

```
 ~/projects/myapp   main   ↑2 ↓0
┌──────────────────────────────────────────────────────┐
│ Files           │ @@ -36,7 +36,12 @@                 │
│  M src/app.rs   │  fn collect_hunks(                  │
│ M  src/diff.rs  │ -    mut on_file: impl FnMut(...),  │
│▶MM src/main.rs  │ +    on_file: impl FnMut(...)       │
├──────────────────────────────────────────────────────┤
│ [1] claude  [2] aider  [3] bash                      │
│ $ cargo test                                         │
└──────────────────────────────────────────────────────┘
 j/k: scroll | /: search | v: view file | <prefix> q: detach
```

## Install

Install straight from the repository (the built viewer bundle is committed, so
this needs no Node toolchain):

```bash
cargo install --git https://github.com/code0xff/nightcrow --locked
```

Or build a local checkout:

```bash
cargo install --path . --locked
```

Once published to crates.io this will also work:

```bash
cargo install nightcrow --locked
```

Requires Rust 1.85+ (edition 2024). `--locked` builds against the committed
`Cargo.lock` for a reproducible install.

## Usage

```bash
# Start the session. Runs in the foreground until you stop it (Ctrl-C).
# It reopens the repositories from last time — nothing, on a first run.
nightcrow

# ...or run it in the background and get your shell back.
nightcrow -d

# From another terminal: bring up the TUI on that session.
nightcrow attach

# Launch terminal panes running commands at startup (repeatable)
nightcrow --exec "claude" --exec "codex"
```

The session prints the address of its browser view (`http://127.0.0.1:8091/`
by default) and the socket an attaching terminal uses. Both show the same
repositories; open one with `<prefix> o` in the TUI or the folder picker in the
browser, and it appears in the other. There is no flag for opening a
repository — a session several clients share has one sensible place to do it,
and that is inside.

If the connection to the session ends under it — the session was stopped, or it
dropped a client that fell too far behind — the TUI leaves and says so, with a
non-zero status. What it had selected and scrolled is written back either way, so
reattaching returns to it. There is no automatic reconnect: reattach when the
session is up.

Leaving the TUI (`<prefix> q`) detaches: the session, and everything running in
its terminals, keeps going. Stopping the session is stopping the process you
started it in — or `kill`ing it, if you used `-d`, in which case its output is
in `~/.nightcrow/daemon.out`. Under a service manager, start it *without* `-d`:
backgrounding is what the manager does itself.

Startup panes belong to a project, not to the process: each project you open
gets its own set. So `nightcrow --exec claude` with no repositories starts
`claude` in the first project opened, not before there is one to open it in.

`--exec` panes open after any `[[startup_command]]` panes from the config
file; the two sources share a combined cap of 8 panes — the same count the
`<prefix> 3`–`9`,`0` jump keys address, so every startup pane is reachable by
a direct key. (`<prefix> 1`/`2` map to the file list and diff viewer.) Panes
opened later with `<prefix> t` are not capped; any past the eighth are reached
by focus cycling (`Shift+←/→`).

## Projects

One nightcrow process holds up to **10 repositories at once**, each in its own
tab across the top row. A project owns everything scoped to its repo — the git
views, the snapshot worker, and its own set of terminal panes — so switching
tabs swaps the whole screen, not just the diff. A pane running a build in one
project keeps running while you work in another.

```
 F1 nightcrow  F2 api-server  +3          ← project tabs (active one accented)
┌ ^F 1 Files ──────┐┌ ^F 2 src/main.rs ────┐
```

- `^F o` opens a repo in a tab, `^F x` closes the active one, and `F1`…`F10`
  switch between them. There is no "change this tab's repo": closing and
  opening is the same thing, and it tears the old project down properly
  instead of leaving its shells behind in the previous directory.
- Opening a repo another tab already holds focuses that tab instead of running
  two copies against one worktree.
- When the tabs outgrow the row, it scrolls around the active tab and folds the
  rest behind `+N` markers; clicking a marker jumps to the nearest project
  behind it.

**No project open** is a normal state, not an error — it is how a fresh
session starts, and where closing the last tab returns you. The screen
keeps its chrome and offers the only two things that apply: `^F o` to open a
repo, `^F q` to quit.

Each project keeps its own session file (see
[Session persistence](#session-persistence)), so tabs restore independently.

## Views

**Status view** (default) — lists changed files on the left, syntax-highlighted diff on the right.

Each row begins with a two-character `XY` status code, following Git's short status notation (nightcrow reads status through git2 internally, not by parsing `git status --short`). `X` is the staged (index) state and `Y` is the unstaged (working-tree) state, so a file can show both at once:

| Code | Meaning |
| --- | --- |
| ` M` | modified, unstaged |
| `M ` | modified, staged |
| `MM` | modified, staged **and** further modified in the working tree |
| `A ` | added (staged) |
| `D `/` D` | deleted (staged / unstaged) |
| `R ` | renamed (shown as `old -> new`; searchable by either path) |
| `T ` | type changed (e.g. file ↔ symlink) |
| `??` | untracked |
| `UU` | conflicted (placeholder for unmerged paths) |

The diff for a selected file shows the combined working-tree-with-index changes.

**Commit log view** (`<prefix> l`) — tig-like commit list on the left, full commit diff on the right. Commits ahead of the upstream are marked with `↑`. Press `Enter` on a commit to drill into its individual files; `Esc` to go back. The list auto-refreshes when the workdir HEAD changes (commits made in the terminal pane, amends, force-pushes, branch switches). History loads one page at a time — initial entry fetches `commit_log_page_size` commits and additional pages stream in on a background thread as the selection approaches the loaded tail, so deep histories stay responsive. Toggling while a terminal or diff pane is zoomed exits the zoom and focuses the list, so the view switch is always visible.

**Tree view** (`<prefix> b`) — a read-only directory tree of the whole working tree on the left, with the selected file's raw contents on the right. Unlike the status view (which lists only changed files), the tree lets you browse and read *any* file next to the diff without leaving nightcrow. `j`/`k` move the cursor, `→`/`Enter` expand a directory (read lazily, one level at a time), `←` collapses it or steps up to the parent, and selecting a file previews it. Press `/` while the tree is focused for a recursive filename search across the whole tree — type to filter, `Enter` reveals the selected match in place (expanding its ancestor directories), `Esc` cancels. Focus the file preview with `<prefix> 2`, then press `/` to search within the file contents — `n`/`N` jump to the next/previous match, `Esc` clears the search. `.gitignore`-matched paths (e.g. `target/`, `node_modules/`) are hidden by default — toggle with `[tree] respect_gitignore`. Expanded directories are watched for filesystem changes, so files and folders created, moved, or deleted by another process (an editor, `git`, an LLM CLI) appear without leaving the view; set `[tree] live_watch = false` to refresh only on entry instead. The tree never writes, renames, or deletes anything. Expansion state and the selected path persist across sessions.

**Notice row** — a one-row strip just above the hint bar shows the repo path (home-relative, e.g. `~/projects/myapp`), the current branch, and ahead/behind counts (`↑N ↓M`) when the branch tracks an upstream. When something fails — a git snapshot, a diff load, a terminal pane, or a repo path you typed that doesn't exist — the message takes over this row in red until the problem is resolved or you act on the app again. A rejected repo path therefore appears directly above the input you're correcting. The repo dialog's completion candidates share this row (dimmed, and a notice outranks them), so a list too long for one line ends in `+N more`.

**Path completion in the repo dialog** — `Tab` completes the directory you're typing, so you don't have to know the path by heart. One press extends as far as the names allow; when there's nothing left to extend it lists what's there instead. On a trailing `/` the first press shows that directory's contents, and a unique match gains a trailing `/` so you can keep pressing `Tab` to descend. Only directories are offered (a file can't be a repo), dotted directories stay hidden until you type a leading `.`, and a name that differs only in case is matched and corrected for you. The dialog is a path field, not a shell — `~`, `..` and paths relative to your working directory all work, but `cd`, `$VAR`, and globs don't, and `Enter` always means "open this path".

**Browsing for a repo** — when you don't know the path, press `↓` in the repo dialog to browse instead of typing. (A second `Tab`, once the candidate list is up, opens the same browser: at that point the flat list has told you all it can.) The browser fills the body of the screen, rooted at whatever directory the field currently names, and the field stays visible below it with the keys spelled out.

| Key | Action |
|-----|--------|
| `↓` / `j`, `↑` / `k` | Move the cursor |
| `→` | Expand the selected directory (read lazily, one level at a time) |
| `←` | Collapse it, or step out — to the parent row, or one level *above the root* when you're already at the top, so a sibling checkout is one press away |
| `Enter` | Take the selected path into the field and return to it — this does **not** open the repo. Press `Enter` again in the field for that, or keep refining the path with `Tab` first |
| `Esc` | Leave the browser, keeping the text it started from. A second `Esc` cancels the dialog |

Directories only, hidden ones excluded, and nothing is ever written. Note that `Enter` means *select* here but *open* in the field — the browser's job is to fill the field, so `→` alone expands (unlike the file-tree view, where `Enter` expands too). Paths keep your own notation: browsing out of `~/coding` gives you back `~/coding/…`, not an absolute path. Mouse selection isn't supported; the browser is keyboard-only.

## Keyboard shortcuts

nightcrow uses a tmux-style **leader (prefix)** key for its app commands. The
default leader is `Ctrl+F` (configurable via `[input] leader`). `Ctrl+F` is a
one-handed left-hand chord that avoids tmux's own `Ctrl+B` prefix (so nightcrow
stays usable inside a tmux session), terminal flow control (`Ctrl+Q`/`Ctrl+S`),
the shell signals (`Ctrl+C/D/Z`), and the Ctrl chords an inner Claude Code pane
reserves (`Ctrl+G` is its external editor, plus `Ctrl+O/R/S/T/L`) — its only
claimant is `Ctrl+F` as forward-char/page-forward, which most users reach via
the arrow keys instead. Press the leader, then a single follow-up key. Every
other key — including Ctrl chords like `Ctrl+W` and `Ctrl+L` — passes straight
through to the focused terminal, so a CLI running there (claude, codex, your
shell) receives them unchanged. This is why the leader exists: cockpit users
live inside the terminal panes and need their prompt-editing keys to reach the
program, not nightcrow.

The hint bar shows the active leader in caret notation at its left edge (e.g.
`^F: leader` for the default `Ctrl+F`), so the configured prefix is always
visible from the terminal pane.

> **Migration from earlier versions:** the old bare-`Ctrl` app shortcuts moved
> behind the leader. `Ctrl+T/W/L/O/P/Q` are now `<prefix> t/w/l/o/p/q` and pass
> through to the terminal program instead; `Ctrl+F` is now the leader itself
> (`<prefix> f` toggles fullscreen). The old `Ctrl+Q`-twice quit confirmation is
> gone; quit with `<prefix> q`.

### Leader commands (press `<prefix>`, then the key)

| Key | Action |
|-----|--------|
| `<prefix>` then `<prefix>` | Send the literal leader to the terminal program |
| `<prefix> t` | Open new terminal pane |
| `<prefix> w` | Close active terminal pane — terminal focus only, since without it no pane is highlighted as the close target |
| `<prefix> s` then `3`…`9`,`0` | Swap the active terminal pane with pane 1…8 (focus follows the pane; same pane numbering as the jump keys, so in terminal fullscreen the swap digits are `1`…`8`) — terminal focus only, like `w`, and needs at least two panes |
| `<prefix> z` | Resize this project's terminal panes to fit this screen. The session gives the sizing to whichever client attached most recently — a PTY has one size, and a program drawing on an alternate screen cannot be re-flowed afterwards — so while another client (a second terminal, or the browser) holds it, this one renders that grid: padded if it is smaller than the pane, cropped if larger. Advertised in the hint bar only while that is the case |
| `<prefix> c` | Give up on the recovery a plugin has pending for a pane — the held slot is released, so nothing can be relaunched into it, and every attached client is told. Targets the focused pane's recovery, or the pane whose process has already ended while its slot was being held (that pane has no tab to focus). Advertised in the hint bar only while something is actually pending |
| `<prefix> l` | Toggle between status view and commit log view |
| `<prefix> b` | Toggle the read-only file-tree view (returns to status view) |
| `<prefix> f` | Fullscreen the focused pane. For the terminal it cycles `off → grid (all panes) → zoom (active pane only) → off`; with a single pane it toggles straight off/on. File list and diff viewer toggle off/on |
| `<prefix> o` | Open a repo in a **project tab** (prefilled with the active project's path — type to replace it, or press `→`/`End` first to extend it). `Tab` completes the path against your filesystem and `↓` opens a directory browser (see below). A leading `~` expands to your home directory. If another tab already has that repo open, nightcrow focuses that tab instead of running two copies against one worktree |
| `<prefix> x` | Close the active project tab. Closing the last one leaves nightcrow with no project open, which is a normal state |
| `<prefix> p` | Cycle accent color (yellow → cyan → green → magenta → blue). The accent belongs to the session, so every attached TUI and every open browser follows |
| `<prefix> u` | Re-read `config.toml` without restarting the session. `[[plugin]]` is re-applied to every open project immediately; `[[startup_command]]` applies to projects you open afterwards, because the panes an open project already started are live processes. Everything else in the file still needs a restart. The result appears on the notice row — see [Reloading the config](#reloading-the-config) |
| `<prefix> r` | Force a full redraw (clears stray glyphs left by terminal programs) |
| `<prefix> q` | Quit |
| `<prefix> 1` / `<prefix> 2` | Focus the file/commit list / diff viewer — **split view only** |
| `<prefix> 3`…`<prefix> 9`, `<prefix> 0` | Jump to terminal pane 1…8 (`0` addresses pane 8) |
| `<prefix> 1`…`<prefix> 8` (terminal fullscreen) | Jump to terminal pane 1…8. With the viewer hidden the digit row addresses panes by natural numbering; `9`/`0` are unused. The only way back to the list/diff is `<prefix> f` to leave fullscreen |
| `Esc` / `Ctrl+C` (while armed) | Cancel the prefix |

The prefix has no timeout: once armed it waits indefinitely for the follow-up
key. A key with no leader binding cancels the prefix and is dropped.

`<prefix> s` is the one two-step chord: it arms a swap mode (shown as `SWAP` in
the hint bar) that waits for a pane digit, then swaps the active pane with the
chosen one. A non-digit follow-up or `Esc` cancels swap mode without reordering.

### Global (no prefix)

| Key | Action |
|-----|--------|
| `Shift+→` / `Shift+←` | Cycle focus: file list → diff viewer → terminal panes → … |
| `F1`…`F10` | Switch to project tab 1…10 — see [Projects](#projects). Unlike the pane digits, this mapping does not change with the layout: the same F-key reaches the same project in every view, fullscreen included |

A modified F-key (`Ctrl+F1`, `Shift+F5`, …) is not intercepted and passes
through to the terminal program.

### File list / Commit list (left panel)

| Key | Action |
|-----|--------|
| `↑` / `k`, `↓` / `j` | Navigate items one by one |
| `PgUp` / `PgDn` | Jump 10 items |
| `←` / `→` | Scroll long paths and commit summaries horizontally (in tree view these expand/collapse instead) |
| `<prefix> f` | Zoom the list pane to full screen (toggle) |
| `/` | Incremental search (status: paths; log: commit summaries; drill-down: paths; tree: recursive filenames) |
| `Esc` | Clear filter, then exit drill-down (log), then cancel search bar |
| `Enter` | Confirm filter (keeps query) or drill into commit's file list (log view) |

### Diff viewer (right panel)

| Key | Action |
|-----|--------|
| `↑` / `k`, `↓` / `j` | Scroll one line |
| `PgUp` / `PgDn` | Scroll 20 lines |
| `←` / `→` | Horizontal scroll (4 columns) |
| `v` | Toggle between hunk diff and full file preview |
| `w` | Toggle soft wrapping of long lines. On, the tail of a long line continues on the next row instead of needing `←`/`→`; the line number folds into the line rather than sitting in its own column, so a continuation row carries no number. Horizontal scrolling is inert while wrapping (and the offset resets when you turn it on). The split view ignores wrapping — halves folding to different heights would stop lining up |
| `Tab` | Cycle the display: unified diff → side-by-side split → file contents → unified. `v` and `s` each toggle one view against the unified default, so the third stays hidden unless you know it exists; `Tab` walks all three. Skips the file step when there is no file to open, and does nothing in tree view |
| `s` | Toggle between the unified diff and a side-by-side split view (falls back to unified when the pane is too narrow) |
| — | **Line numbers** are always shown in a pinned gutter. The unified view shows both sides (old, new) — an added line leaves the old column blank, a removed line leaves the new one blank. The split view numbers each half with the side it shows, and the file view (`v`) numbers the file itself. The gutter stays in place while `←`/`→` scroll the code |
| `<prefix> f` | Zoom the diff/file pane to full screen (toggle) |
| `/` | Open search (works in both diff and file preview, including tree mode) |
| `n` / `N` | Next / previous search match |
| `Esc` | Clear search |

### Terminal panes (bottom)

Every visible pane renders at once as a split grid instead of switching
between tabs — 2 panes go side by side (or stacked if the terminal is
narrow), 4 form a 2x2 grid, up to 4 show normally and up to 8 in the
fullscreen grid. `<prefix> f` cycles the terminal through `off → grid →
zoom → off`: *grid* hides the top viewer and fills the screen with the
split grid, *zoom* fills the screen with just the active pane. The active
pane's cell is bordered in the accent color; jumping focus with `<prefix> 3`–`9`,`0`
or `Shift+←/→` moves that border (and, while zoomed, the pane on screen)
without closing any other pane. With more panes than fit, the tab bar
shows a `+N` marker for the ones scrolled out of view — they keep running
in the background. Keyboard input, paste, and scroll still target only the
active pane. A single pane draws with no cell border, exactly as before
split view existed.

| Key | Action |
|-----|--------|
| `Shift+↑` / `Shift+↓` | Scroll terminal output 3 lines |
| `Shift+PgUp` / `Shift+PgDn` | Scroll terminal output one page |

While scrolled, the terminal border title shows `[SCROLL — shift+pgdn: down | input: live]`. Keyboard input is still forwarded to the running process; `Shift+PgDn` to scroll back to the bottom.

The tab bar picks up OSC 0/2 window-title escape sequences, so programs like `claude`, `vim`, `ssh`, or `cd`-aware shell prompts can rename their own tab. Panes without an emitted title fall back to a default label.

### Mouse

nightcrow captures the mouse by default (`[mouse]` in the configuration):

- **Click a pane** to focus it, same as a jump key. The click is also forwarded to programs that asked for mouse reports (Claude Code, `less --mouse`, …) — so their clickable UI, like Claude Code's jump-to-bottom control, works. A plain shell receives nothing.
- **Click the file list or diff viewer** to focus that panel, same as `<prefix> 1`/`2`.
- **Click a project tab** in the top row to switch to it, same as its `F`-key. A `+N` overflow marker jumps to the nearest project folded behind it.
- **Wheel** scrolls the pane under the pointer, routed exactly like the scroll keys (wheel reports, arrow keys, or scrollback — whatever the program expects).
- **Click a tab** in the terminal tab bar to jump to that pane; clicking a `+N` hidden-pane marker reveals the nearest hidden pane on that side.
- **Click `o: open project`** on the empty screen — with no project open it is the one action the hint bar offers, and it dispatches like its key.
- **Click a shortcut** in the bottom hint bar to run it — command hints like `t: new pane`, `w: close pane`, or `f: fullscreen` dispatch exactly as if you pressed the keys they name. Clickable hints render inverted (reverse video) across their whole label so they stand out from informational hints; the inversion disappears when `[mouse]` is disabled. Navigation hints and `q: quit` are not clickable (quitting stays a deliberate two-key act).
- **Select text with a bypass modifier + drag.** While the mouse is captured, the outer terminal performs its native selection and copy only when you hold its bypass modifier while dragging. The modifier depends on the terminal: **Shift** in xterm-family terminals (Alacritty, kitty, GNOME Terminal, Windows Terminal), **Option (⌥)** in iTerm2, **Fn or Option** in macOS Terminal.app. Set `enabled = false` under `[mouse]` to give the mouse back to the outer terminal entirely — plain-drag selection returns, click forwarding stops.

## Recent-activity focus indicator

Files modified within the last `hot_window_secs` seconds — whether by an agent in a terminal pane, your editor, or a build/format script — are rendered in the accent color (bold for the first 5 seconds, normal until the window expires). When the file list is in focus and you have not navigated in the last 2 seconds, the selection auto-follows to the freshest hot file so the diff updates as files change. Manual navigation (`j` / `k` / arrows / PgUp / PgDn) immediately suppresses auto-follow until you go idle again.

Configurable under `[agent_indicator]` (see below).

## Session persistence

nightcrow saves the current state on exit and restores it on the next launch — focus position, selected file, scroll offset, active terminal pane, view mode (status / commit log / tree), fullscreen states, commit-log drill-down position, and tree expansion and selection.

The accent is not in that list. It belongs to the session rather than to one repo's view state, so it lives in `~/.nightcrow/viewer.json` alongside the viewer's other shared preferences and is not restored per repo.

Everything else lands in one file, `~/.nightcrow/workspace.json` — which repos were open, which tab was in front, and each repo's view state. Nothing is written inside your repositories: no single repo owns the fact that others were open beside it, and nightcrow shouldn't create directories in a project it is only reading.

A bare `nightcrow` reopens those tabs and lands on the one that was in front, with each project's selection and scroll where you left them. Repos that have moved or been deleted since are skipped, with a notice saying how many. View state is kept for the 50 most recently used repos.

The two halves have two owners. The session writes which repositories are open and which tab is in front; an attached client writes what it had selected and scrolled, and never the tab list — detaching must not roll the session back to one client's view of it. To start empty, close every tab before stopping the session.

## Plugins

nightcrow itself knows nothing about the CLIs you run in its panes — an agent
and a person get the same PTY. Behaviour that *does* need to know a particular
tool lives in a plugin: a separate executable that nightcrow launches and talks
to over a pipe. Plugins are off unless you turn one on, and one only ever sees a
pane you handed it by name — or, if you also set `watch_on_signal`, a pane that
something running inside it spoke to the plugin from. A plugin is never given a
list of your panes either way.

The bundled plugin is `nightcrow-recovery`. When a watched pane's CLI hits its
usage limit, it waits for the reset time the provider reported and then re-opens
that exact session. It only waits — it does not bypass, raise, or work around
any provider limit, and it sends nothing while a limit is in effect. Claude
Code, Codex CLI, and OpenCode are supported; OpenCode is only ever observed,
never interrupted, because it retries on its own.

```bash
cargo build --release -p nightcrow-recovery
nightcrow plugin install target/release/nightcrow-recovery --name recovery
nightcrow plugin list        # what is installed, and how config refers to it
nightcrow plugin remove recovery
```

`install` prints the exact `[[plugin]]` block to paste, using whatever `--name`
you chose — that name is what a pane opts in with, so keep the two in step.

Installing only puts the binary in `~/.nightcrow/plugins`. It stays inert until
you edit `~/.nightcrow/config.toml` yourself — enabling something that can type
into a terminal should be a change you read before it takes effect:

```toml
[[plugin]]
name = "recovery"
command = "nightcrow-recovery"
enabled = true
# Flags the plugin may append to re-open a session. Empty by default, which
# refuses every relaunch. nightcrow cannot know what a CLI's flags mean, so it
# will not add one you did not list — that is what keeps a plugin from changing
# how a CLI asks for your approval.
allowed_resume_flags = ["--resume", "resume", "--session"]

[[startup_command]]
name = "Claude"
command = "claude"
plugin = "recovery"      # without this line, no plugin sees this pane unless
                         # watch_on_signal is set (see below)
```

That covers the panes you configured. For the pane you did not — you opened a
shell with `<prefix> t` and typed `claude` into it yourself — add
`watch_on_signal = true` to the `[[plugin]]` block. nightcrow puts a random token
in each pane's environment and nowhere else, so the CLI's own hook can quote it
back and the plugin can ask for "the pane this token names"; a plain shell never
speaks to a plugin, so your shells stay untouched. It is off by default. Such a
pane can be waited for and typed into but never relaunched — nightcrow launched
no command in it, so there is nothing to put back.

For Claude Code, let the plugin install its hook and statusline entries so it
can read the exact session id and reset time instead of guessing from what is
printed on screen. With a reset time it waits exactly once; without one it falls
back to retrying on a backoff, which can give up. It merges into your existing
`~/.claude/settings.json` and backs it up first:

```bash
nightcrow-recovery install-hooks
nightcrow-recovery uninstall-hooks    # removes only what it added
```

Claude Code's `statusLine` holds one command, so installing does replace yours —
but it is then run from the plugin's own statusline with the same input, and what
it prints is what you see. `uninstall-hooks` puts it back.

A pane that is waiting shows its state and deadline on its tab. Cancel it with
`<prefix>` then the recovery key (see [Leader commands](#leader-commands-press-prefix-then-the-key)),
or from the web viewer; typing into the pane yourself also cancels it.

Design and trust boundary: [`docs/architecture.md`](docs/architecture.md) → "Plugin Host".

## Web viewer

A browser surface that renders the same git data as a native web page —
selectable text, real scrolling, clickable paths, and a layout that adapts to a
phone (see below). It also serves its **own** terminals, independent of the
TUI's panes.

The served repositories appear as project tabs in the header — `+ open` browses
the server machine's folders to add one, `×` closes it, and dragging a tab
reorders them. The same dialog **clones a git URL** into the folder it is
showing: paste `https://…` or `git@host:path`, and the repository opens as a
tab when the clone finishes. Cloning runs `git` on the server, so it uses that
machine's credentials — an SSH agent, a credential helper — and a private
remote works exactly as it would in a shell there. Local paths and git's
`ext::` transport are refused. A clone keeps running whether or not you stay to
watch it: closing the dialog leaves `Cloning…` in the header, and a page you
reload — or a phone that dropped the tab mid-transfer — picks the same clone
back up and still opens the repository when it lands. Each project has its own `status`, `log`, and `tree` tabs on
the left plus a terminal panel below. The order is kept on the server, so every
device shows the same arrangement, and it survives a
restart (alongside the TUI it lasts the session). On a narrow window (phone) the
tab row folds into a dropdown showing the current project.

In the `log` tab, selecting a commit opens its changed-file list alongside the
complete commit diff. Select a file to view only that file's change; use
`< log` to return or `all changes` to restore the complete commit diff.

History loads a page at a time, as the TUI's does — scrolling toward the end of
the list fetches the next page, so deep histories stay reachable without loading
them up front. The filter narrows the commits already loaded rather than
searching the server, so paging pauses while a query is up — the list says how
many are loaded, and clearing the filter resumes loading. The list is the history as of entering the tab: unlike
the TUI it does not follow HEAD, so a commit made in the terminal panel appears
after leaving and re-entering the tab.

The swatch in the header cycles the accent colour through the same five
presets as the TUI's `<prefix> p` (yellow → cyan → green → magenta → blue) —
and it is the same colour, not a parallel one. The choice is stored on the
server (`~/.nightcrow/viewer.json`), so every device that opens the viewer and
every attached TUI shows it, and a change made anywhere reaches the browsers
within a few seconds and attached terminals immediately. `[theme] name` sets
the colour a session starts with, before anyone has picked one.

Drag the divider between the file list and the diff pane to resize the sidebar,
or double-click it to reset the default width. The width is stored on the server
the same way as the accent, so every device opens at the same split; it is
bounded so the diff pane always keeps at least half the window.

The border between the diff panel and the terminal panel is a divider too: drag
it to give the terminal more or less of the window, double-click to go back to
the default 55/45. It is stored on the server like the sidebar width, so every
browser opens at the same split, and bounded so neither panel shrinks to a
sliver — for "all the way" use the maximize buttons on either panel. Unlike the
accent, this one is **not** shared with an attached TUI: the TUI keeps its own
`[layout] upper_pct`, because the same percentage means a different number of
rows on a terminal than in a browser window, and the terminals' actual size is
already decided by whichever client owns the sizing.

The diff pane has a toggle (top-right of the pane) that switches between the
inline unified diff and a side-by-side split view, mirroring the TUI's `s`.
The choice lasts the page, the same lifetime the TUI gives it; on a narrow
window (phone) the two sides stack — removed above added — rather than sitting
side by side, since neither column would have the width to read.

**Line numbers** ride in a pinned gutter as they do in the TUI: the unified
view shows both sides (old, new), leaving a column blank where the line does
not exist on that side; each split half shows the side it renders; and a file
opened from the tree is numbered by its own lines. The gutter stays put while
the code scrolls sideways, and the numbers stay out of anything you copy.

Drag a terminal pane by its header onto another to reorder the split-view grid;
it works with touch as well as a mouse. The order is kept on the server, so a
refresh, a reconnect, or another device opening the same repository all show the
same arrangement. (It is not written to disk — a server restart clears the
terminals themselves, so there is nothing to persist.)

**On a phone**, the three regions the desktop shows at once — the file/commit
list, the content pane, and the terminal — would each shrink to an unusable
sliver stacked in one column, so instead a bottom bar switches between them:
tap **Files**, **Diff**, or **Terminal** to give one of them the whole screen.
Opening a file or commit jumps to the content view automatically. Because a
soft keyboard can't type Escape, Tab, Shift-Tab, Ctrl combinations, or the
arrows, the terminal grows a key bar along its bottom on touch devices that
sends those straight to the shell — so you can interrupt a process (`^C`),
leave `vim` (`Esc`), cycle a completion menu backwards (`⇧Tab`), or walk your
history (arrows) without a physical keyboard.

The viewer ships a web-app manifest and icons, so you can **add it to your home
screen** and launch it as a standalone, chrome-less window — more room for the
terminal and one-tap access. On iOS this works over plain HTTP (Safari →
*Share* → *Add to Home Screen*). Android's install prompt additionally wants a
service worker and a secure origin, so reach the viewer over HTTPS (a reverse
proxy or tunnel) to get it there; the viewer has no offline mode either way —
every screen needs the server.

The `status` list highlights recently touched files the same way the TUI does:
accent-coloured and bold for the first 5 seconds after a file's mtime, accent
until `agent_indicator.hot_window_secs` expires, then plain. The window (and
whether the highlight runs at all) comes from the server's `[agent_indicator]`
settings, so both surfaces fade on the same schedule. Ageing is measured
against the browser's clock, so a device whose time is badly off will fade
early or late.

Markdown files (`.md`, `.markdown`) opened from the tree render as formatted
documents by default, with fenced code syntax-highlighted. HTML files
(`.html`, `.htm`) render too, inside a fully sandboxed frame: scripts do not
run, and nothing loads from another host. A page that carries its own styling
inline and embeds images as `data:` URIs shows in full; one that links a
stylesheet or images as separate files shows without them, since repository
files are not served to the frame. This previews a self-contained page rather
than a site. A toggle (top-right of the
pane) switches either back to the raw highlighted source.

It is always on — it is one of the session's two faces, not an add-on. Configure
where it listens under `[web_viewer]`:

```toml
[web_viewer]
bind = "127.0.0.1"   # loopback only; change deliberately
port = 8091
# password = "..."   # auto-generated and written here on first launch if unset
```

`--port` and `--bind` override those for one run:

```bash
nightcrow --port 9000
```

Repositories opened or closed in the browser reach every attached terminal, and
are written back to `~/.nightcrow/workspace.json` so the next session starts on
the same set.

**Authentication.** If no `password` is set when the viewer is enabled, a random
one is generated and written back into your config (so it survives restarts and
stays readable) and printed once at startup. To avoid a plaintext password on
disk, set `hashed_password` to an Argon2 PHC string instead — it takes
precedence. Login is rate-limited and grants a session cookie.

> **Security.** The viewer serves repository contents *and* interactive
> terminals, so an authenticated session is equivalent to shell access. It binds
> to loopback (`127.0.0.1`) by default and speaks plain HTTP with **no built-in
> TLS**. For remote access, do **not** expose the port directly — tunnel it over
> SSH (`ssh -L 8091:127.0.0.1:8091 host`) or put it behind a TLS reverse proxy.

### Developing the frontend

The UI lives in `viewer-ui/` (React + Vite + Tailwind). Its build output is
committed to `viewer-ui/dist/` and embedded into the binary, so installing
nightcrow never requires Node.

```bash
npm --prefix viewer-ui install
npm --prefix viewer-ui run dev     # Vite on :5173, proxying the API to :8091
npm --prefix viewer-ui run build   # rebuild dist/ — commit the result
```

CI rebuilds the bundle and fails if it differs from what is committed.

## Configuration

Config file: `~/.nightcrow/config.toml` (all fields optional, defaults shown).
nightcrow runs on built-in defaults when the file is absent and never creates
it on its own. To get a starter file, run:

```bash
nightcrow init            # writes a commented ~/.nightcrow/config.toml
nightcrow init --force    # overwrite an existing file
```

`init` leaves an existing config untouched unless `--force` is passed.

```toml
[layout]
upper_pct = 55       # vertical % for the diff panel (1–99) — the TUI's own; the
                     # viewer keeps a separate dragged value in viewer.json
file_list_pct = 25   # horizontal % of upper panel for the file list (1–99)

[theme]
name = "yellow"      # accent a session starts with, before anyone picks one:
                     # "yellow" | "cyan" | "green" | "magenta" | "blue"

[input]
leader = "ctrl+f"    # leader (prefix) chord for app commands; tmux-style.
                     # Allowed: "ctrl+<letter>". Reserved keys (F1..F10,
                     # Shift+arrows, Shift+PgUp/PgDn) cannot be the leader.

[mouse]
enabled = true       # capture the mouse: click to focus/forward, wheel scrolls
                     # the pane under the pointer; select text with the
                     # terminal's bypass modifier + drag (Shift in xterm-family,
                     # Option in iTerm2, Fn/Option in macOS Terminal.app).
                     # false = plain-drag selection, no click forwarding.

[web_viewer]
bind = "127.0.0.1"   # loopback only by default; plain HTTP, so tunnel/proxy for remote
port = 8091
# password = "..."         # auto-generated + saved here on first launch if unset
# hashed_password = "..."  # Argon2 PHC string; takes precedence over `password`

[log]
enabled = true
dir = ".nightcrow/logs"   # relative paths resolve under the home directory
rotation = "daily"        # "daily" | "hourly" | "size"
max_size_mb = 10          # used when rotation = "size"
max_days = 7              # delete logs older than N days (0 = keep forever)
level = "info"            # "error" | "warn" | "info" | "debug" | "trace"
prompt_log = false        # record terminal prompt input line by line
commit_log_page_size = 100        # commits fetched per commit-log page
commit_log_prefetch_threshold = 25 # start the next-page fetch when the selection is within
                                  # this many rows of the loaded tail (1..=page_size)

[agent_indicator]
enabled = true            # color recently-touched files in the file list
hot_window_secs = 15      # seconds within which a file stays hot (3–3600)
auto_follow = false       # jump selection to the freshest hot file when idle

# Read-only directory-tree navigator (enter with <prefix> b).
[tree]
respect_gitignore = true  # hide .gitignore-matched paths (target/, node_modules/, …)
max_depth = 64            # deepest directory level the tree will expand into (1..=1024)
live_watch = true         # watch expanded dirs and refresh the tree live; set false
                          # to refresh only on tree entry (large trees / odd filesystems)

# Reserve startup commands: each [[startup_command]] opens its own terminal
# pane at launch and runs `command` immediately (via `$SHELL -lc <command>`).
# Up to 8 entries (combined with CLI --exec). 8 matches the <leader> 3–9,0
# jump keys, so every startup pane is reachable by a direct key (<leader> 1/2
# reach the file list and diff viewer). This caps only the startup batch — open
# more anytime with <leader> t (panes past the eighth are reached by focus
# cycling, Shift+←/→). `name` labels the tab; when omitted the command text is
# used. With no [[startup_command]] entries, nightcrow opens a single empty shell.
[[startup_command]]
name = "Claude"           # optional tab label; falls back to the command text
command = "claude"        # required; must not be empty
plugin = "recovery"       # optional; names the [[plugin]] allowed to act on this
                          # pane. Omitted — the default — means no plugin sees
                          # it unless that plugin sets watch_on_signal.

[[startup_command]]
command = "cargo test --watch"

# External plugin processes — see "Plugins" above. Up to 8 entries, names unique.
# Nothing runs unless an entry exists AND enabled = true AND either a pane opted
# in or watch_on_signal is set.
[[plugin]]
name = "recovery"                 # the name panes opt in with
command = "nightcrow-recovery"    # found on PATH or in ~/.nightcrow/plugins
args = []                         # passed to the plugin verbatim
enabled = false                   # off by default
watch_on_signal = false           # off by default; when true, a pane no
                                  # [[startup_command]] named is also handed over
                                  # once something inside it quotes that pane's
                                  # token to this plugin. Such a pane is never
                                  # relaunched, only typed into while it lives.
allowed_resume_flags = []         # flags the plugin may append to re-open a
                                  # session; empty refuses every relaunch

[plugin.env]                      # plugin process only, never terminal panes
NIGHTCROW_RECOVERY_LOG = "info"
```

### Reloading the config

Editing `config.toml` normally means restarting the session — which kills every
pane, including whatever an agent CLI was in the middle of. Two of the tables
can be re-read instead, without stopping anything:

- **In the TUI**: `<prefix> u`. The result appears on the notice row.
- **In the browser**: the ⟳ button in the header, next to sign out. It reloads
  the *config*, not the page — nothing on screen changes, and the result comes
  back as a toast.

What a reload applies:

| Table | When it takes effect |
| --- | --- |
| `[[plugin]]` | **Immediately, in every open project.** Newly enabled plugins start and are handed the panes that opted into them; disabled or removed ones stop and their panes carry on unwatched. A plugin whose `command`, `args` or `env` changed gets a new process; changing only `allowed_resume_flags` or `watch_on_signal` leaves the running one alone, so a plugin part-way through a long wait is not disturbed |
| `[[startup_command]]` | **On the next project you open.** A project that is already open keeps the panes it started with — those are live processes, and no file edit replaces them |
| Everything else | Needs a restart: `[web_viewer]` (the listener is already bound), `[log]`, and the client-owned `[layout]`, `[input]`, `[tree]`, `[mouse]` sections, which each TUI reads when it attaches |

Notes:

- **Nothing half-applies.** The whole file is parsed and validated first, so a
  typo anywhere leaves the session exactly as it was, and the message names the
  key that was wrong.
- **A missing file is refused** rather than read as "nothing is configured" —
  otherwise deleting the file and reloading would be a quiet way to stop every
  plugin.
- Panes opened with `--exec` are kept: they are not in the file, so a reload
  merges them back where a restart would have put them.
- Disabling a plugin and enabling it again lands where enabling it the first
  time would — the pane's opt-in survives, so `enabled` means the same thing
  whichever way it was last flipped.

## License

Apache License 2.0. See [LICENSE](LICENSE).
