# nightcrow

TUI for Agentic Coding — git diff viewer + commit log + multi-terminal panes in one terminal window.

```
┌──────────────────────────────────────────────────────┐
│ Files           │ @@ -36,7 +36,12 @@                 │
│ M src/app.rs    │  fn collect_hunks(                  │
│ M src/diff.rs   │ -    mut on_file: impl FnMut(...),  │
│▶M src/main.rs   │ +    on_file: impl FnMut(...)       │
├──────────────────────────────────────────────────────┤
│ [1] claude  [2] aider  [3] bash                      │
│ $ cargo test                                         │
└──────────────────────────────────────────────────────┘
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

**Commit log view** (`Ctrl+L`) — tig-like commit list on the left, full commit diff on the right. Commits ahead of the upstream are marked with `↑`. Press `Enter` on a commit to drill into its individual files; `Esc` to go back.

## Keyboard shortcuts

### Global

| Key | Action |
|-----|--------|
| `Shift+→` / `Shift+←` | Cycle focus: file list → diff viewer → terminal panes → … |
| `Ctrl+L` | Toggle between status view and commit log view |
| `Ctrl+T` | Open new terminal pane |
| `Ctrl+W` | Close active terminal pane |
| `F1`…`F9` | Jump to terminal pane N |
| `Ctrl+F` | Toggle fullscreen for the focused pane (diff viewer or terminal) |
| `Ctrl+P` | Cycle accent color (yellow → cyan → green → magenta → blue) |
| `Ctrl+O` | Change repo path |
| `Ctrl+Q` | Quit |

### File list / Commit list (left panel)

| Key | Action |
|-----|--------|
| `↑` / `k`, `↓` / `j` | Navigate items one by one |
| `PgUp` / `PgDn` | Jump 10 items |
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

## Agent-aware focus indicator

Files modified within the last `hot_window_secs` seconds are tagged with `★ ` in the file list and rendered in the accent color (bold for the first 3 seconds, normal until the window expires). When the file list is in focus and you have not navigated in the last 2 seconds, the selection auto-follows to the freshest hot file so the diff updates as the agent works. Manual navigation (`j` / `k` / arrows / PgUp / PgDn) immediately suppresses auto-follow until you go idle again.

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
prompt_log = false        # record terminal prompt input line by line

[agent_indicator]
enabled = true            # show the ★ marker on recently-touched files
hot_window_secs = 10      # seconds within which a file stays hot (3–3600)
auto_follow = true        # jump selection to the freshest hot file when idle
```

## License

MIT
