# nightcrow

TUI for Agentic Coding — git diff viewer + multi-terminal panes in one terminal window.

```
┌─────────────────────────────────────────────────┐
│ src/app.rs          │  - fn poll_terminal(...)   │  ← diff viewer (syntect)
│ src/ui/mod.rs       │  + fn poll_snapshot(...)   │
│▶src/backend/pty.rs  │    ...                     │
├─────────────────────────────────────────────────┤
│ [1] claude  [2] aider  [3] bash                 │  ← terminal panes
│ $ claude --continue                             │
└─────────────────────────────────────────────────┘
```

## Install

```bash
cargo install nightcrow
```

Requires Rust 1.85+ (edition 2024).

Optional: `tmux` in `$PATH` for multi-pane support (falls back to PTY if absent).

## Usage

```bash
# Open in current git repo
nightcrow

# Open a specific repo
nightcrow --repo ~/projects/myapp

# Show version / help
nightcrow --version
nightcrow --help
```

## Keyboard shortcuts

| Key | Action |
|-----|--------|
| `Tab` | Toggle focus: file list ↔ diff viewer ↔ terminal |
| `←` / `→` / `Shift+Tab` | Switch focus within upper panel |
| `j` / `k` or `↑` / `↓` | Navigate file list |
| `PgUp` / `PgDn` | Scroll diff view |
| `Ctrl+T` | Open new terminal pane |
| `Ctrl+1`…`9` | Switch to terminal pane N |
| `q` | Quit (from diff/file panel) |
| `Ctrl+Q` | Quit (from terminal panel) |

## Configuration

Create `~/.config/nightcrow/config.toml` to override defaults:

```toml
[layout]
upper_pct = 55      # percentage of screen for the diff panel
file_list_pct = 25  # percentage of the upper panel for the file list

[keys]
quit = "q"
focus_toggle = "Tab"
new_pane = "ctrl-t"
```

## License

MIT
