# nightcrow documentation

The [top-level README](../README.md) is the tour: what nightcrow is, how to install it, and enough usage to get a session up. Everything past that lives here, one page per surface.

## Using nightcrow

| Page | What it covers |
| --- | --- |
| [Getting started](getting-started.md) | Install, starting and stopping a session, attaching, startup panes |
| [Projects](projects.md) | Repository tabs, the empty state, per-project scope |
| [Views](views.md) | Status, commit log, and tree views; the notice row; the repo dialog and its directory browser |
| [Keyboard and mouse](keybindings.md) | The leader key, every binding, mouse routing |
| [Session state](session-state.md) | Recent-activity highlighting, what persists across restarts and who owns it |
| [Web viewer](web-viewer.md) | The browser surface, phone layout, authentication, frontend development |
| [Plugins](plugins.md) | The plugin boundary and the bundled `nightcrow-recovery` |
| [Configuration](configuration.md) | Every `config.toml` table, and which ones reload without a restart |

## Working on nightcrow

| Page | What it covers |
| --- | --- |
| [Architecture](architecture.md) | Index: overview, layout, module map, stack — and links into the detail pages below |
| [· Session](architecture/session.md) | Daemon ↔ client split, `TerminalBackend`, PTY size ownership, config reload |
| [· Git views](architecture/git-views.md) | Diff pipeline, gutter and wrapping, tree navigator, commit-log decoration |
| [· Terminal](architecture/terminal.md) | Split-view pane grid, emulation layer, scroll and mouse routing |
| [· UI](architecture/ui.md) | Keyboard routing, the `Workspace`/`App` project boundary, notice row |
| [· Plugin host](architecture/plugin-host.md) | The trust boundary and the recovery surface |
| [· Web layer](architecture/web.md) | Shared HTTP/SSE primitives, the viewer, the frontend |
| [Design decisions](decisions.md) | Why it went this way — rejected alternatives and where implementation diverged from plan |
| [AGENTS.md](../AGENTS.md) | Contribution workflow and repository conventions |
