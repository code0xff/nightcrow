# nightcrow

Agent-adjacent terminal workbench — git diff viewer, commit log, and multi-pane terminal multiplexer in one window. Tuned for sitting next to LLM CLIs (Claude Code, Codex, aider) or any process that touches your working tree, but nightcrow itself has no AI ontology — it watches files and PTYs, not agents.

```
 ~/projects/myapp   main   ↑2 ↓0
┌──────────────────────────────────────────────────────┐
│ Files           │ @@ -36,7 +36,12 @@                 │
│ M src/app.rs    │  fn collect_hunks(                  │
│ M src/diff.rs   │ -    mut on_file: impl FnMut(...),  │
│▶M src/main.rs   │ +    on_file: impl FnMut(...)       │
├──────────────────────────────────────────────────────┤
│ [1] claude  [2] aider  [3] bash                      │
│ $ cargo test                                         │
└──────────────────────────────────────────────────────┘
 j/k: scroll | /: search | v: view file | ctrl+q: quit
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
```

## Views

**Status view** (default) — lists changed files on the left, syntax-highlighted diff on the right.

**Commit log view** (`Ctrl+L`) — tig-like commit list on the left, full commit diff on the right. Commits ahead of the upstream are marked with `↑`. Press `Enter` on a commit to drill into its individual files; `Esc` to go back. The list auto-refreshes when the workdir HEAD changes (commits made in the terminal pane, amends, force-pushes, branch switches). History loads one page at a time — initial entry fetches `commit_log_page_size` commits and additional pages stream in on a background thread as the selection approaches the loaded tail, so deep histories stay responsive.

**Top header** — a one-row strip at the top of the screen always shows the repo path (home-relative, e.g. `~/projects/myapp`), the current branch, and ahead/behind counts (`↑N ↓M`) when the branch tracks an upstream.

## Keyboard shortcuts

### Global

| Key | Action |
|-----|--------|
| `Shift+→` / `Shift+←` | Cycle focus: file list → diff viewer → terminal panes → … |
| `Ctrl+L` | Toggle between status view and commit log view |
| `Ctrl+T` | Open new terminal pane |
| `Ctrl+W` | Close active terminal pane |
| `F1` / `F2` | Focus file list / diff viewer |
| `F3`…`F9` | Jump to terminal pane 1…7 |
| `Ctrl+F` | Toggle fullscreen for the focused pane (file/commit list, diff viewer, or terminal) |
| `Ctrl+P` | Cycle accent color (yellow → cyan → green → magenta → blue) |
| `Ctrl+O` | Change repo path |
| `Ctrl+Q` | Quit |

### File list / Commit list (left panel)

| Key | Action |
|-----|--------|
| `↑` / `k`, `↓` / `j` | Navigate items one by one |
| `PgUp` / `PgDn` | Jump 10 items |
| `Ctrl+F` | Zoom the list pane to full screen (toggle) |
| `/` | Incremental search (status view only) |
| `Esc` | Cancel search |
| `Enter` | Drill into commit's file list (log view) |
| `Esc` | Return to commit list from file drill-down |

### Diff viewer (right panel)

| Key | Action |
|-----|--------|
| `↑` / `k`, `↓` / `j` | Scroll one line |
| `PgUp` / `PgDn` | Scroll 20 lines |
| `←` / `→` | Horizontal scroll (4 columns) |
| `v` | Toggle between hunk diff and full file preview |
| `Ctrl+F` | Zoom the diff/file pane to full screen (toggle) |
| `/` | Open diff search |
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

Config file: `~/.config/nightcrow/config.toml` (all fields optional, defaults shown).

```toml
[layout]
upper_pct = 55       # vertical % for the diff panel (1–99)
file_list_pct = 25   # horizontal % of upper panel for the file list (1–99)

[theme]
name = "yellow"      # accent color preset: "yellow" | "cyan" | "green" | "magenta" | "blue"

[log]
enabled = true
dir = ".nightcrow/logs"   # relative to repo root
rotation = "daily"        # "daily" | "hourly" | "size"
max_size_mb = 10          # used when rotation = "size"
max_days = 7              # delete logs older than N days (0 = keep forever)
level = "info"            # "error" | "warn" | "info" | "debug" | "trace"
prompt_log = true         # record terminal prompt input line by line
commit_log_page_size = 300        # commits fetched per commit-log page (200–500)
commit_log_prefetch_threshold = 50 # start the next-page fetch when the selection is within
                                  # this many rows of the loaded tail (1..=page_size)

[agent_indicator]
enabled = true            # color recently-touched files in the file list
hot_window_secs = 15      # seconds within which a file stays hot (3–3600)
auto_follow = true        # jump selection to the freshest hot file when idle

# Reserve startup commands: each [[startup_command]] opens its own terminal
# pane at launch and runs `command` immediately (via `$SHELL -lc <command>`).
# Up to 9 entries (aligned with the F1–F9 pane-jump keys). `name` labels the
# tab; when omitted the command text is used. With no [[startup_command]]
# entries, nightcrow opens a single empty shell as before.
[[startup_command]]
name = "Claude"           # optional tab label; falls back to the command text
command = "claude"        # required; must not be empty

[[startup_command]]
command = "cargo test --watch"
```

## License

MIT
