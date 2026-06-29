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
# Open in current git repo
nightcrow

# Open a specific repo
nightcrow --repo ~/projects/myapp

# Launch terminal panes running commands at startup (repeatable)
nightcrow --exec "claude" --exec "codex"
```

`--exec` panes open after any `[[startup_command]]` panes from the config
file; the two sources share a combined cap of 7 panes — the same count the
`F3`–`F9` (or `<prefix> 3`–`9`) jump keys address, so every startup pane is
reachable by a direct key. (`F1`/`F2` map to the file list and diff viewer.)
Panes opened later with `<prefix> t` are not capped; any past the seventh are
reached by focus cycling (`Shift+←/→`).

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

**Tree view** (`<prefix> b`) — a read-only directory tree of the whole working tree on the left, with the selected file's raw contents on the right. Unlike the status view (which lists only changed files), the tree lets you browse and read *any* file next to the diff without leaving nightcrow. `j`/`k` move the cursor, `→`/`Enter` expand a directory (read lazily, one level at a time), `←` collapses it or steps up to the parent, and selecting a file previews it. Focus the file preview with `F2`, then press `/` to search within the file contents — `n`/`N` jump to the next/previous match, `Esc` clears the search. `.gitignore`-matched paths (e.g. `target/`, `node_modules/`) are hidden by default — toggle with `[tree] respect_gitignore`. Expanded directories are watched for filesystem changes, so files and folders created, moved, or deleted by another process (an editor, `git`, an LLM CLI) appear without leaving the view; set `[tree] live_watch = false` to refresh only on entry instead. The tree never writes, renames, or deletes anything. Expansion state and the selected path persist across sessions.

**Top header** — a one-row strip at the top of the screen always shows the repo path (home-relative, e.g. `~/projects/myapp`), the current branch, and ahead/behind counts (`↑N ↓M`) when the branch tracks an upstream.

## Keyboard shortcuts

nightcrow uses a tmux-style **leader (prefix)** key for its app commands. The
default leader is `Ctrl+G` (configurable via `[input] leader`). `Ctrl+G` avoids
tmux's own `Ctrl+B` prefix, so nightcrow stays usable inside a tmux session. Press the
leader, then a single follow-up key. Every other key — including Ctrl chords
like `Ctrl+W` and `Ctrl+L` — passes straight through to the focused terminal,
so a CLI running there (claude, codex, your shell) receives them unchanged.
This is why the leader exists: cockpit users live inside the terminal panes and
need their prompt-editing keys to reach the program, not nightcrow.

The hint bar shows the active leader in caret notation at its left edge (e.g.
`^G: leader` for the default `Ctrl+G`), so the configured prefix is always
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
| `<prefix> w` | Close active terminal pane |
| `<prefix> l` | Toggle between status view and commit log view |
| `<prefix> b` | Toggle the read-only file-tree view (returns to status view) |
| `<prefix> f` | Toggle fullscreen for the focused pane (file/commit list, diff viewer, or terminal) |
| `<prefix> o` | Change repo path |
| `<prefix> p` | Cycle accent color (yellow → cyan → green → magenta → blue) |
| `<prefix> r` | Force a full redraw (clears stray glyphs left by terminal programs) |
| `<prefix> q` | Quit |
| `<prefix> 1` / `<prefix> 2` | Focus the file/commit list / diff viewer (mirrors `F1` / `F2`) |
| `<prefix> 3`…`<prefix> 9` | Jump to terminal pane 1…7 (mirrors `F3`…`F9`) |
| `Esc` / `Ctrl+C` (while armed) | Cancel the prefix |

The prefix has no timeout: once armed it waits indefinitely for the follow-up
key. A key with no leader binding cancels the prefix and is dropped.

### Global (no prefix)

| Key | Action |
|-----|--------|
| `Shift+→` / `Shift+←` | Cycle focus: file list → diff viewer → terminal panes → … |
| `F1` / `F2` | Focus file list / diff viewer |
| `F3`…`F9` | Jump to terminal pane 1…7 |

### File list / Commit list (left panel)

| Key | Action |
|-----|--------|
| `↑` / `k`, `↓` / `j` | Navigate items one by one |
| `PgUp` / `PgDn` | Jump 10 items |
| `<prefix> f` | Zoom the list pane to full screen (toggle) |
| `/` | Incremental search (status: paths; log: commit summaries; drill-down: paths) |
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

| Key | Action |
|-----|--------|
| `Shift+↑` / `Shift+↓` | Scroll terminal output 3 lines |
| `Shift+PgUp` / `Shift+PgDn` | Scroll terminal output one page |

While scrolled, the terminal border title shows `[SCROLL — shift+pgdn: down | input: live]`. Keyboard input is still forwarded to the running process; `Shift+PgDn` to scroll back to the bottom.

The tab bar picks up OSC 0/2 window-title escape sequences, so programs like `claude`, `vim`, `ssh`, or `cd`-aware shell prompts can rename their own tab. Panes without an emitted title fall back to a default label.

## Recent-activity focus indicator

Files modified within the last `hot_window_secs` seconds — whether by an agent in a terminal pane, your editor, or a build/format script — are rendered in the accent color (bold for the first 5 seconds, normal until the window expires). When the file list is in focus and you have not navigated in the last 2 seconds, the selection auto-follows to the freshest hot file so the diff updates as files change. Manual navigation (`j` / `k` / arrows / PgUp / PgDn) immediately suppresses auto-follow until you go idle again.

Configurable under `[agent_indicator]` (see below).

## Session persistence

nightcrow saves the current state on exit and restores it on the next launch for the same repo — focus position, scroll offset, active terminal pane, commit log view mode, and accent color. The state file is `.nightcrow/session.json` inside the repo directory.

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
leader = "ctrl+g"    # leader (prefix) chord for app commands; tmux-style.
                     # Allowed: "ctrl+<letter>". Reserved keys (F1..F9,
                     # Shift+arrows, Shift+PgUp/PgDn) cannot be the leader.

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

# Reserve startup commands: each [[startup_command]] opens its own terminal
# pane at launch and runs `command` immediately (via `$SHELL -lc <command>`).
# Up to 7 entries (combined with CLI --exec). 7 matches the F3–F9 / <leader>
# 3–9 jump keys, so every startup pane is reachable by a direct key (F1/F2
# reach the file list and diff viewer). This caps only the startup batch — open
# more anytime with <leader> t (panes past the seventh are reached by focus
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
