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
| `Shift+→` | Cycle focus forward: file list → diff viewer → terminal panes → … |
| `Shift+←` | Cycle focus backward |
| `↑` / `↓` or `k` / `j` | Navigate file list; scroll diff view line by line |
| `PgUp` / `PgDn` | Jump 10 files in file list; scroll diff view 20 lines |
| `/` | Search files (file list focus) |
| `Esc` | Cancel search or repo input |
| `Ctrl+T` | Open new terminal pane |
| `Ctrl+W` | Close active terminal pane |
| `F1`…`F9` | Jump to terminal pane N directly |
| `Ctrl+O` | Change repo path |
| `Ctrl+Q` | Quit |

## Configuration

Create `~/.config/nightcrow/config.toml` to override defaults:

```toml
[layout]
upper_pct = 55      # percentage of screen for the diff panel
file_list_pct = 25  # percentage of the upper panel for the file list
```

## License

MIT
