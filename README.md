# nightcrow

Agent-adjacent terminal workbench: inspect Git changes while running several terminal programs beside them. A session owns the repositories and PTYs; a TUI and the browser viewer can attach to the same session. Closing either client leaves the session running.

nightcrow is a single Rust binary for macOS, Linux, and Windows.

## Quick start

### 1. Install

```bash
cargo install --git https://github.com/code0xff/nightcrow --locked
```

Rust 1.85 or newer is required. To create an editable starter configuration first, run `nightcrow init`; see [Configuration](docs/configuration.md).

### 2. Start a session

```bash
nightcrow attach
```

`attach` starts a background session when none is running, then opens the TUI. If a session already exists it attaches to that session instead of starting another one. The session prints its browser URL and the path used by later `nightcrow attach` commands. The web viewer uses a generated password on first start and prints it once; see [Web viewer](docs/web-viewer.md#access-and-security).

With the default leader (`Ctrl+F`), press the leader and then:

- `<prefix> o` opens a repository as a project tab.
- `<prefix> t` opens a terminal pane.
- `<prefix> l` and `<prefix> b` switch to the commit log and tree views.
- `<prefix> f` toggles fullscreen and `<prefix> q` detaches the TUI.

The complete reference is [Keyboard and mouse](docs/keybindings.md).

### 3. Stop or update

```bash
nightcrow stop       # stop the running session and its terminal programs
nightcrow update     # reinstall the binary; restart the session afterwards
```

For foreground operation, use `nightcrow`; `nightcrow -d` starts the session in the background and writes its output to `~/.nightcrow/daemon.out`. See [Getting started](docs/getting-started.md) for installation variants, startup panes, disconnects, updates, and build verification.

To inspect a running daemon without attaching, run `nightcrow status [--socket PATH]`. It performs a read-only one-shot query and reports the PID, version, start time, uptime, endpoint, attached clients, repositories, and panes. It exits non-zero when no daemon is running.

## Features

- Up to 10 repository tabs, each with its own Git views and terminal panes → [Projects](docs/projects.md).
- Status, commit log, and read-only tree views → [Views](docs/views.md).
- Shared session state, recent-activity highlighting, and restart behavior → [Session state](docs/session-state.md).
- Configurable layout, input, shell, logging, startup commands, plugins, and web access → [Configuration](docs/configuration.md).
- A browser surface for the same repositories and interactive terminals → [Web viewer](docs/web-viewer.md).
- Optional external plugins, including bundled recovery that waits out Codex/OpenCode usage limits and reopens exact sessions → [Plugins](docs/plugins.md).

## Security

The authenticated web viewer exposes repository contents and interactive terminals, which is equivalent to shell access. It binds to loopback and uses plain HTTP by default. Do not expose the port directly on a network; use an SSH tunnel or a TLS reverse proxy, and protect the password/configuration files.

## Documentation

The [documentation index](docs/README.md) routes to user guides and the separate [architecture](docs/architecture.md) and [design decisions](docs/decisions.md) references.

Apache License 2.0. See [LICENSE](LICENSE).
