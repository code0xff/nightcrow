# nightcrow

Agent-adjacent terminal workbench — git diff viewer, commit log, and multi-pane terminal multiplexer in one window. Tuned for sitting next to LLM CLIs (Claude Code, Codex, aider) or any process that touches your working tree, but nightcrow itself has no AI ontology — it watches files and PTYs, not agents.

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
 j/k: scroll | /: search | v: view file | <prefix> q: quit
```

## Install

```bash
cargo install nightcrow
```

Requires Rust 1.85+ (edition 2024).

## Usage

```bash
# Reopen the tabs from last time (empty on a first run)
nightcrow

# Open a repo in a tab
nightcrow --repo ~/projects/myapp

# Open several at once, one tab each (repeatable)
nightcrow --repo ~/projects/api --repo ~/projects/web

# Launch terminal panes running commands at startup (repeatable)
nightcrow --exec "claude" --exec "codex"
```

Startup panes belong to a project, not to the process: each project opened —
by `--repo` at launch or by `^Q o` later — gets its own set. So
`nightcrow --exec claude` with no repo opens the empty screen and starts
`claude` in the first project you open, not before there is one to open it in.

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
┌ ^Q1 Files ──────┐┌ ^Q2 src/main.rs ────┐
```

- `^Q o` opens a repo in a tab, `^Q x` closes the active one, and `F1`…`F10`
  switch between them. There is no "change this tab's repo": closing and
  opening is the same thing, and it tears the old project down properly
  instead of leaving its shells behind in the previous directory.
- Opening a repo another tab already holds focuses that tab instead of running
  two copies against one worktree.
- When the tabs outgrow the row, it scrolls around the active tab and folds the
  rest behind `+N` markers; clicking a marker jumps to the nearest project
  behind it.

**No project open** is a normal state, not an error — it is how nightcrow
starts without `--repo`, and where closing the last tab returns you. The screen
keeps its chrome and offers the only two things that apply: `^Q o` to open a
repo, `^Q q` to quit.

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

**Notice row** — a one-row strip just above the hint bar shows the repo path (home-relative, e.g. `~/projects/myapp`), the current branch, and ahead/behind counts (`↑N ↓M`) when the branch tracks an upstream. When something fails — a git snapshot, a diff load, a terminal pane, or a repo path you typed that doesn't exist — the message takes over this row in red until the problem is resolved or you act on the app again. A rejected repo path therefore appears directly above the input you're correcting.

## Keyboard shortcuts

nightcrow uses a tmux-style **leader (prefix)** key for its app commands. The
default leader is `Ctrl+Q` (configurable via `[input] leader`). `Ctrl+Q` avoids
both tmux's own `Ctrl+B` prefix (so nightcrow stays usable inside a tmux session)
and the Ctrl chords an inner Claude Code pane reserves (`Ctrl+G` is its external
editor, plus `Ctrl+O/R/S/T/L`) — its only other claimant is terminal flow
control (XON), which nightcrow's raw-mode input disables. Press the
leader, then a single follow-up key. Every other key — including Ctrl chords
like `Ctrl+W` and `Ctrl+L` — passes straight through to the focused terminal,
so a CLI running there (claude, codex, your shell) receives them unchanged.
This is why the leader exists: cockpit users live inside the terminal panes and
need their prompt-editing keys to reach the program, not nightcrow.

The hint bar shows the active leader in caret notation at its left edge (e.g.
`^Q: leader` for the default `Ctrl+Q`), so the configured prefix is always
visible from the terminal pane.

> **Migration from earlier versions:** the old bare-`Ctrl` app shortcuts moved
> behind the leader. `Ctrl+T/W/L/F/O/P/Q` are now `<prefix> t/w/l/f/o/p/q`, and
> those `Ctrl` keys now pass through to the terminal program instead. The old
> `Ctrl+Q`-twice quit confirmation is gone; quit with `<prefix> q`.

### Leader commands (press `<prefix>`, then the key)

| Key | Action |
|-----|--------|
| `<prefix>` then `<prefix>` | Send the literal leader to the terminal program |
| `<prefix> t` | Open new terminal pane |
| `<prefix> w` | Close active terminal pane — terminal focus only, since without it no pane is highlighted as the close target |
| `<prefix> s` then `3`…`9`,`0` | Swap the active terminal pane with pane 1…8 (focus follows the pane; same pane numbering as the jump keys, so in terminal fullscreen the swap digits are `1`…`8`) — terminal focus only, like `w`, and needs at least two panes |
| `<prefix> l` | Toggle between status view and commit log view |
| `<prefix> b` | Toggle the read-only file-tree view (returns to status view) |
| `<prefix> f` | Fullscreen the focused pane. For the terminal it cycles `off → grid (all panes) → zoom (active pane only) → off`; with a single pane it toggles straight off/on. File list and diff viewer toggle off/on |
| `<prefix> o` | Open a repo in a **project tab** (prefilled with the active project's path — type to replace it, or press `→`/`End` first to extend it). A leading `~` expands to your home directory. If another tab already has that repo open, nightcrow focuses that tab instead of running two copies against one worktree |
| `<prefix> x` | Close the active project tab. Closing the last one leaves nightcrow with no project open, which is a normal state |
| `<prefix> p` | Cycle accent color (yellow → cyan → green → magenta → blue) |
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
- **Select text with Shift+drag.** While the mouse is captured, the outer terminal performs its native selection and copy only with Shift held (standard behavior in every major terminal). Set `enabled = false` under `[mouse]` to give the mouse back to the outer terminal entirely — plain-drag selection returns, click forwarding stops.

## Recent-activity focus indicator

Files modified within the last `hot_window_secs` seconds — whether by an agent in a terminal pane, your editor, or a build/format script — are rendered in the accent color (bold for the first 5 seconds, normal until the window expires). When the file list is in focus and you have not navigated in the last 2 seconds, the selection auto-follows to the freshest hot file so the diff updates as files change. Manual navigation (`j` / `k` / arrows / PgUp / PgDn) immediately suppresses auto-follow until you go idle again.

Configurable under `[agent_indicator]` (see below).

## Session persistence

nightcrow saves the current state on exit and restores it on the next launch — focus position, selected file, scroll offset, active terminal pane, view mode (status / commit log / tree), fullscreen states, commit-log drill-down position, tree expansion and selection, and accent color.

Everything lands in one file, `~/.nightcrow/workspace.json` — which repos were open, which tab was in front, and each repo's view state. Nothing is written inside your repositories: no single repo owns the fact that others were open beside it, and nightcrow shouldn't create directories in a project it is only reading.

A bare `nightcrow` reopens those tabs and lands on the one that was in front, with each project's selection and scroll where you left them. Repos that have moved or been deleted since are skipped, with a notice saying how many. View state is kept for the 50 most recently used repos.

Passing `--repo` overrides the remembered tab list — it names the repo you want, so restoring extra tabs beside it would be surprising. Each repo's view state is still restored. To start empty, close every tab before quitting: that records an empty list, which is what the next launch restores.

## Web mirror

nightcrow can serve a live, controllable view of itself in a browser. The
browser and the local terminal drive the **same** session and stay in sync —
type on either side, watch both update. It is off by default; enable it under
`[web_mirror]`:

```toml
[web_mirror]
enabled = true
bind = "127.0.0.1"   # loopback only; change deliberately
port = 8090
# password = "..."   # auto-generated and written here on first launch if unset
```

On launch nightcrow prints the address (e.g. `http://127.0.0.1:8090/`). Open it,
sign in with the password, and the page renders the exact TUI — file list, diff,
commit log, and terminal panes — via [xterm.js](https://xtermjs.org/). Keyboard,
mouse, and paste all work: the leader chord, focus jumps, terminal input, and
pane clicks behave identically to the local terminal because browser input runs
through the same handlers. The local terminal stays the authority for the grid
size; the browser scales that grid to fit its window.

**Authentication.** If no `password` is set when the server is enabled, a random
one is generated and written back into your config (so it survives restarts and
stays readable) and printed once at startup. To avoid a plaintext password on
disk, set `hashed_password` to an Argon2 PHC string instead — it takes
precedence. Login is rate-limited and grants a session cookie.

> **Security.** Enabling the web mirror grants live control of a shell. It binds
> to loopback (`127.0.0.1`) by default and speaks plain HTTP with **no built-in
> TLS**. For remote access, do **not** expose the port directly — tunnel it over
> SSH (`ssh -L 8090:127.0.0.1:8090 host`) or put it behind a TLS reverse proxy.

## Web viewer

A second, different browser surface: instead of mirroring the TUI's screen, it
renders the same git data as a native web page — selectable text, real
scrolling, clickable paths, and a layout that collapses to one column on a
phone. It also serves its **own** terminals, independent of the TUI's panes.

In the Log tab, selecting a commit opens its changed-file list alongside the
complete commit diff. Select a file to view only that file's change; use
`← log` to return or `all changes` to restore the complete commit diff.

The swatch in the header cycles the accent colour through the same five
presets as the TUI's `<prefix> p` (yellow → cyan → green → magenta → blue).
It is stored per browser and is **not** read from `[theme]` — that setting
colours the TUI, and the viewer keeps its own.

Enable it alongside the TUI under `[web_viewer]`:

```toml
[web_viewer]
enabled = true
bind = "127.0.0.1"   # loopback only; change deliberately
port = 8091
# password = "..."   # auto-generated and written here on first launch if unset
```

Or run it with no TUI at all:

```bash
nightcrow serve --repo ~/code/app --repo ~/code/api
nightcrow serve --repo . --port 9000
```

`serve` runs in the foreground until Ctrl-C. Because the server never touches
the TUI's state, it needs no terminal — which is what makes this mode possible.

**Mirror or viewer?** The mirror is for *driving* nightcrow remotely: it is the
same session, so what you type appears on both screens. The viewer is for
*reading* a repository comfortably in a browser, with terminals of its own. They
run on separate ports with separate passwords and separate session cookies, and
either can be enabled without the other.

**Authentication.** `[web_viewer]` has its own credential, generated and written
back on first launch exactly like `[web_mirror]`. A mirror session does not
authenticate against the viewer, or the reverse.

> **Security.** The viewer serves repository contents *and* interactive
> terminals, so an authenticated session is equivalent to shell access. The same
> rules as the mirror apply: loopback by default, no built-in TLS, and remote
> access belongs behind an SSH tunnel or a TLS reverse proxy.

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
upper_pct = 55       # vertical % for the diff panel (1–99)
file_list_pct = 25   # horizontal % of upper panel for the file list (1–99)

[theme]
name = "yellow"      # accent color preset: "yellow" | "cyan" | "green" | "magenta" | "blue"

[input]
leader = "ctrl+q"    # leader (prefix) chord for app commands; tmux-style.
                     # Allowed: "ctrl+<letter>". Reserved keys (F1..F10,
                     # Shift+arrows, Shift+PgUp/PgDn) cannot be the leader.

[mouse]
enabled = true       # capture the mouse: click to focus/forward, wheel scrolls
                     # the pane under the pointer; select text with Shift+drag.
                     # false = plain-drag selection, no click forwarding.

[web_mirror]
enabled = false      # serve a live, controllable mirror in a browser (off by default)
bind = "127.0.0.1"   # loopback only by default; plain HTTP, so tunnel/proxy for remote
port = 8090
# password = "..."         # auto-generated + saved here on first launch if unset
# hashed_password = "..."  # Argon2 PHC string; takes precedence over `password`

[web_viewer]
enabled = false      # serve the native web viewer (off by default)
bind = "127.0.0.1"   # loopback only by default
port = 8091
# password = "..."         # separate credential from the mirror above
# hashed_password = "..."  # Argon2 PHC string; takes precedence over `password`

[log]
enabled = true
dir = ".nightcrow/logs"   # relative to repo root
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

[[startup_command]]
command = "cargo test --watch"
```

## License

Apache License 2.0. See [LICENSE](LICENSE).
