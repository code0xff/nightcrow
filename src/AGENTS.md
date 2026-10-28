# `src/` Scope

This guide applies only to the Rust core in `src/`. Follow the [root guide](../AGENTS.md) for repository-wide rules and the [architecture index](../docs/architecture.md) for design invariants. This guide records only the non-obvious boundaries in `src/`.

## Core boundaries

- `session/` is daemon-owned, transport-neutral state. `application/` and `web/` are clients that translate their inputs and protocols into session operations; they do not own repositories, the terminal hub, shared preferences, or PTY sizing. See [session design](../docs/architecture/session.md) for the session boundary.
- The attached TUI's daemon socket transport and the browser viewer's HTTP/WebSocket transport are separate security boundaries. The former relies on socket-file permissions and the latter on web authentication. Keep shared state changes in `session/`, but do not combine authentication or wire handling for the two transports. See [web design](../docs/architecture/web.md) for the web layer.
- `TerminalBackend` joins the local `PtyBackend` to the daemon-shared session's `HubBackend`. Observe pane creation, closure, reordering, and resizing through the backend event contract; actual PTY sizing follows session-level ownership and confirmed `Resized` events. Do not bypass this boundary by having a frontend update PTY or hub internals directly.

## Source organization

- `app.rs` is the crate-internal TUI facade; preserve its existing `pub` and `pub(crate)` visibility. Group internal Git view handling under `app/git/` and focus/input navigation under `app/navigation/`. Keep application wiring and session I/O at the root of `app/`, and place new behavior at its nearest responsibility boundary so the flat module set does not grow again.

## Protocol and platform seams

- Daemon frames distinguish control JSON from raw terminal bytes. If changing framing kinds, length validation, truncated-stream handling, or terminal-payload chunking invariants, update [session design](../docs/architecture/session.md) and the corresponding wire/contract tests. Keep session repository-set notifications behind a single watcher producer so competing client responses cannot reorder them.
- Put OS-dependent behavior behind the existing `platform/` seam, and hide Unix/Windows differences in daemon socket types in one place, `daemon/transport.rs`. Before scattering new `cfg` branches in callers, check whether the existing seam can absorb them. Document platform behavior that has no equivalent.
