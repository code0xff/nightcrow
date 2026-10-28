# nightcrow Architecture

This document indexes the current design. It keeps the overall picture and cross-cutting invariants; area-specific contracts live in the documents under [`docs/architecture/`](architecture/). Design rationale and rejected alternatives are recorded once in [`decisions.md`](decisions.md).

## Overview

nightcrow is an agent-adjacent Rust application composed of one session daemon and multiple frontends. Running `nightcrow` without arguments starts the daemon and browser viewer together; the TUI attaches to the same session separately with `nightcrow attach`. The daemon owns the repository set, terminal panes, and shared preferences, while each client renders its own screen. The daemon and panes remain alive if a client exits, so clients can reconnect. Both surfaces use the same session capabilities; differences in screen geometry, lifecycle, and input model are described in the relevant detailed documents.

The upper area shows repository status/diff, commit log, and a read-only tree; the lower area shows multiple PTYs at once. The TUI and browser read the same repository and terminal state, but each keeps its own display state, such as geometry, cursor, scroll, and search. The core does not interpret AI providers; provider-specific behavior belongs in process-isolated plugins.

```text
filesystem/git ──> per-repository runtime ──> session daemon
                                             ├── local attach TUI
                                             └── authenticated web viewer
```

## Ownership and data flow

| Boundary | Owns | Exposes |
| --- | --- | --- |
| `session/` | catalog, repo runtime, terminal hub/PTY, active repo/order/accent, PTY size ownership | transport-neutral operations and runtime events |
| `application/` + `app/` | TUI `Workspace`/`App`, focus/geometry/search/scroll/local view state | daemon socket requests and ratatui rendering |
| `web/` + `viewer-ui/` | HTTP/WebSocket auth/wire, browser geometry, and viewer preferences | JSON/SSE/terminal binary and DOM rendering |
| `plugin/` | provider-specific detection/recovery processes | bounded NDJSON events/commands |

The TUI's [`app.rs`](../src/app.rs) is a crate-internal facade that preserves the visibility of its current items. The implementation groups Git views under `src/app/git/` and focus/input navigation under `src/app/navigation/`; application wiring and session I/O stay at the root of the app boundary. Other layers follow the ownership boundaries above, with detailed contracts in their respective documents.

The repository catalog separates membership (paths, order, hidden paths, and opaque IDs) from runtime (workers and terminal hubs). A catalog transaction computes membership and reconciles runtime. Entries for unchanged paths are retained to avoid replacing watchers, SSE streams, and hubs unnecessarily; retired workers are stopped after releasing the catalog lock.

Per-repository snapshot workers read status in response to filesystem changes and do not read or watch when there are no subscribers. An on-demand `/api/status` request is the only exception and may read once without subscribers. A status payload represents the latest complete state and can be conflated, but terminal bytes form an ordered stream and cannot be dropped. Git diff/file/log selection loads are coalesced by lane in long-lived workers that own `git2::Repository`; results are discarded when `(repository, generation)` no longer matches the current intent. The tree lazily reads only the directories needed at the UI boundary.

The terminal hub owns PTYs and sends raw bytes plus lifecycle/control events to clients. Each client applies the same bytes to its own emulator, while the hub emulator produces the current mode/title/screen and `screen + since` replay needed for reconnects. Both attach and WebSocket use bounded queues; terminal queue overflow closes the connection rather than continuing to render a damaged stream.

## Shared state and client state

- Session-wide values are open repositories and their order, active repo, pane set/content/order/confirmed size, and accent. The TUI and browser share the active repo and accent.
- Browser-only shared values are `upper_pct` in `viewer.json`, each project's last view, and maximize arrangement. They are not shared with the TUI because screen dimensions have different meanings. Values that also differ between browsers—pixel sidebar width, pane view mode, key bar, and shortcut leader—are not shared; each browser keeps them in `localStorage`.
- Cursor, scroll, focus, fullscreen, search input, and the TUI's `Workspace` view state are client-local. Terminal attention/read state for hidden projects is also client-local, so activity on one surface does not clear the read state on another. The TUI workspace file and viewer preference file never overwrite each other.
- One owner per session determines PTY size. Direct interaction such as key input, paste, click, or wheel, and an explicit `claim`, can change the owner; new connections, screen-size changes, and redraws preserve the current owner. Two seconds after an owner leaves, ownership passes to the most recently active remaining viewer, or to the most recently connected viewer if none has activity history. Non-owner resizes are discarded; only successfully applied `Resized` events are reflected to everyone.

## Cross-cutting invariants

- **Single source of geometry**: `ui::chrome::chrome_areas` alone computes the four chrome areas (project tabs, body, notice, and hint), and `ui::terminal_tab::visible_pane_cells` alone computes visible pane cells. Rendering, resize, and hit-testing do not perform separate geometry calculations. All four areas always exist, so inserting or deleting rows/columns never resizes PTYs. Project tabs are one row at the top or a fixed-width column on the left according to `[layout] tabs`; this setting is read only when the TUI attaches and cannot be toggled during a session.
- **Input protection**: App commands are available only after the default leader (`Ctrl+F`, configurable); other ordinary keys and standalone Ctrl keys pass through to the active pane. The only exceptions are a small set of chords reserved without a leader: bare F-keys, shift-only arrows/PageUp/PageDown, and `Ctrl+Shift` plus left/right. They match only the exact modifier set; other combinations pass through to the pane. Synthetic scroll/mouse reports are sent only when the program has enabled the corresponding mode.
- **Ordering and generation**: Backend/session events confirm pane creation, exit, resize, and reorder. One watcher sends the daemon's repository set, and terminal output retains per-repository FIFO order. Asynchronous Git results are applied only after passing a generation guard.
- **Path boundaries**: Use `git::path::resolve_in_workdir` to open worktree files and `validate_commit_path` for Git objects/pathspecs. Reject traversal, absolute paths, NUL, and `.git` variants; worktree file access also refuses symlinks in intermediate components. Web routes pass through shared handlers rather than reimplementing validation.
- **Resource bounds and errors**: Frames, terminal queues, PTYs/panes, web connections/responses/lists/diffs/searches have explicit limits. Mark partial results with `truncated` or an equivalent indicator; never report malformed/truncated input or external-call failures as success.
- **Platform seams**: Path, logging, signal, and thread helpers live under `platform/`; daemon socket types live in `daemon/transport.rs`. Unix-only APIs and Windows ConPTY differences stay behind these seams, and details without an equivalent on another platform are documented in the relevant area document.
- **Security boundaries**: Attach sockets rely on file permissions; web auth uses Argon2 passwords and server-side session cookies. Web requests follow Host → Origin → static bundle → authentication → repository lookup → path gate order, and plugin commands go through `Guard` authorization separately from shape validation.

## Detailed design

| Document | Current contract |
| --- | --- |
| [session.md](architecture/session.md) | `TerminalBackend`, daemon/client ownership, catalog/watchers, PTY size owner, replay/backpressure, config reload, worker shutdown |
| [git-views.md](architecture/git-views.md) | diff/file/log/tree pipeline, path gate, line rendering, status cache, HEAD/ref updates |
| [terminal.md](architecture/terminal.md) | pane grid and sizing, VT emulator, scroll/mouse routing |
| [ui.md](architecture/ui.md) | leader routing, `Workspace`/`App`, repo dialog, dirty redraw, chrome/notice |
| [plugin-host.md](architecture/plugin-host.md) | process/NDJSON boundary, opt-in/token guard, relaunch/recovery surface |
| [web.md](architecture/web.md) | web auth/HTTP/SSE/WS, route/path gates, wire fixtures, browser state, clone |

← [Documentation index](README.md)
