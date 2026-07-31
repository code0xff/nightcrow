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

The built viewer bundle is committed, so this needs no Node toolchain:

```bash
cargo install --git https://github.com/code0xff/nightcrow --locked
```

Requires Rust 1.85+ (edition 2024). Other install routes are in
[Getting started](docs/getting-started.md#install).

## Quick start

```bash
# Start the session (foreground; -d to background it).
# It reopens the repositories from last time.
nightcrow

# From another terminal: bring up the TUI on that session.
nightcrow attach
```

The session prints the address of its browser view (`http://127.0.0.1:8091/` by
default) and the socket an attaching terminal uses. Both show the same
repositories — open one with `<prefix> o` in the TUI or the folder picker in the
browser, and it appears in the other.

The leader (prefix) key is `Ctrl+F` by default. Press it, then one key:
`o` opens a repo, `t` a terminal pane, `l` the commit log, `b` the file tree,
`f` fullscreen, `q` quits. Every other key — including Ctrl chords — passes
straight through to the focused terminal, so the CLI running there receives them
unchanged.

## What it does

- **Up to 10 repositories at once**, each a project tab with its own git views,
  snapshot worker, and terminal panes. → [Projects](docs/projects.md)
- **Three views** over each repo — changed files with a syntax-highlighted diff,
  a tig-like commit log, and a read-only file tree you can browse and search.
  → [Views](docs/views.md)
- **A split-grid terminal panel** where every visible pane renders at once, with
  scrollback, OSC title capture, and mouse routing. →
  [Keyboard and mouse](docs/keybindings.md)
- **A browser surface** serving the same git data and the same terminals, with a
  phone layout and an on-screen key bar. → [Web viewer](docs/web-viewer.md)
- **Recent-activity highlighting** — files touched in the last few seconds are
  accented, so you see what an agent just changed.
  → [Session state](docs/session-state.md)
- **Session persistence** — tabs, selection, scroll, and view mode come back on
  the next launch. Nothing is written inside your repositories.
- **Plugins** for behaviour that must know a specific CLI; the bundled
  `nightcrow-recovery` waits out a provider's usage limit and re-opens the
  session. → [Plugins](docs/plugins.md)

Configure it in `~/.nightcrow/config.toml` (`nightcrow init` writes a commented
starter) — see [Configuration](docs/configuration.md).

> **Security.** The web viewer serves repository contents *and* interactive
> terminals, so an authenticated session is equivalent to shell access. It binds
> to loopback and speaks plain HTTP with no built-in TLS. For remote access,
> tunnel it over SSH or put it behind a TLS reverse proxy.

## Documentation

Full docs are in [`docs/`](docs/README.md) — usage guides per surface, the
[architecture](docs/architecture.md), and the
[design-decision history](docs/decisions.md).

## License

Apache License 2.0. See [LICENSE](LICENSE).
