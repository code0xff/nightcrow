# Design Decisions

This document records only the reasons for current design choices and important alternatives. For current behavior and invariants, use [`architecture.md`](architecture.md) and its detailed design documents; for user procedures, use [`README.md`](../README.md).

## Session boundaries

### The daemon owns state and clients render

Having the daemon create and mirror a single ratatui screen would prevent the terminal and browser from viewing the same pane at different sizes. The daemon owns repositories, PTYs, and shared preferences, while each client renders with its own geometry and emulator. This lets the TUI and viewer use the same session operations without a separate screen-mirroring layer.

### Synchronous threads and local Git reads

Keep bounded threads/channels instead of adding an external async runtime. Git diff/file/tree/log reads use the same local worktree through client-side paths; Git data is not moved over the daemon wire for remote attach. Instead, selection loads make the UI asynchronous using a worker that owns `git2::Repository` and a generation guard.

### Single instance and safe daemonization

Connecting to a stale socket cannot reliably determine whether a process is alive when the Unix socket is closed. Use `flock` for the process lock, and start daemon mode by re-executing with `setsid` rather than forking. This avoids forking a process that already has threads and inheriting its lock/thread state in the child.

### Shared values and per-screen values

The active repository and accent are facts shared by the same session, so TUI and browser share them. Cursor, scroll, focus, fullscreen, search, and the display ratio (`upper_pct`) vary by screen and remain client-local. Viewer preferences and the TUI workspace also use separate files so one surface cannot overwrite another surface's view state.

### One last-interacted viewer owns PTY size

PTY children and alternate-screen programs draw to the width they receive, and those screens cannot be merged after the fact. Therefore one size owner exists for each session. A screen claims ownership when a person interacts through key press/repeat, paste, click, touch, or wheel; a new connection, window resize, focus, redraw, or reconnect alone does not transfer it. Repeated interaction by the same owner does not force a resize, while `<leader> r` and the web's manual fit control explicitly reapply geometry. Two seconds after the owner leaves, ownership passes to the most recently active remaining viewer, or to the most recently connected viewer if none has activity history. Non-owners are observers and follow only actual `Resized` events.

## State, streams, and concurrency

### Read on changes; use one producer for repository sets

Use a filesystem watcher instead of reading every worktree every second, with a 1-second timer fallback only if watcher setup fails. Do not read or watch a repository without subscribers. Since HTTP and attach can change the session concurrently, restrict the producer that rereads and sends set/active/accent to `daemon/watch.rs`. Spreading callbacks across mutations would let new paths forget to broadcast; two producers could also reverse frame order.

### Coalesce complete snapshots; preserve terminal bytes

One latest status value is a complete snapshot, so intermediate values can be conflated. Terminal output is an escape-sequence and multibyte stream, so not even one byte can be omitted. Terminal queues are therefore bounded FIFOs; disconnect clients that overflow so they can receive a consistent replay. Replay combines a screen snapshot with subsequent `since` bytes and sends 1 MiB chunks to stay under the daemon frame limit.

### Separate catalog membership from runtime

Combining pure membership calculations with worker/hub lifetimes in one object would recreate unrelated subscribers on tab reorder or config reload. Preserve runtime entries by path, and serialize membership/runtime/config-table changes in one transaction. Join retired workers after releasing the transaction lock so shutdown of one repository cannot block the next mutation.

### Validate the whole config, then apply only supported changes

Do not partially apply `config.toml` to preserve live panes. Parse and validate the whole file first, then apply `[[plugin]]` to open hubs and `[[startup_command]]` plus `[terminal] auto_open` only to new hubs. Plugin permission flags and the watch switch are read from the next decision onward; only command/args/env changes that require a child replacement restart it. Serialize concurrent reloads with a lock, and never report success for a hub that could not receive the change.

## TUI input and Git display

### Put app commands after a leader

To preserve inputs such as `Ctrl+W` and `Ctrl+L` for LLM CLIs and shells, ordinary keys cannot be reserved as global shortcuts. Put app commands after the default `Ctrl+F` leader; bare F-keys and shift-only navigation are the exceptional no-prefix reserved keys. No leader timeout is used, so users do not lose prompt input in nested TUIs.

### Keep chrome rows and Git status notation consistent

Inserting a row for every notice/hint would resize all PTYs and make programs redraw. Always reserve four rows—project tabs, body, notice, and hint—and overlay notices. For status, use the familiar `XY path` notation rather than inventing a new display; model staged and worktree columns with the same `StatusKind`. Keep a rename's effective path separate from its display path, and do not hide typechange/conflict as modified.

### The repo picker browses native paths, not a shell

Launching readline in a PTY does not support Windows and would require distinguishing candidates from results in the PTY stream. A picker based on `std::fs::read_dir` provides the same input model on all three platforms without OS-specific shell dependencies or another protocol. Preserve the entered `~`/relative spelling in the UI and expand it only when reading.

## Viewer and web boundary

### The viewer is a second frontend, not a TUI mirror

Mirroring the TUI grid as an image would make browser geometry and responsive layout difficult. The viewer shares only session Git/runtime/terminal primitives and has its own JSON/SSE/WebSocket/React surface. A daemon started without arguments also starts the viewer, and bundling `viewer-ui/dist` makes `cargo install` work without Node.

### Centralize request order and path gates

Check Host before Origin, serve the static bundle before authentication, and authenticate before repository lookup. Route-specific path validation is easy to omit when a route is added, so restrict file access to `with_repo` and paths passed to Git to `with_repo_git_path`. Do not add overly strict filesystem checks that would also block diffs for deleted files.

### Opaque repository IDs and anchor pagination

Give clients stable opaque IDs for the process lifetime, not absolute paths. Do not use the last commit as the `/api/log` cursor: walking only its ancestors can omit parallel-branch commits in merge history. Use the same revwalk's `from` anchor and `skip` instead.

### Clone uses a Git subprocess and URL allowlist

The vendored libgit2 build lacks SSH transport, credential-helper, and scp-like remote support, so delegate cloning to the `git` binary. Since `ext::` can execute commands, restrict URL schemes to `https/http/ssh/git+ssh` and scp-like forms; reject `file://`, local paths, and `git://`. Reserve the destination with `create_dir` before cloning and keep clone jobs and concurrency bounded.

### Upload pasted images and type their path

The only way to pass an image to a CLI in a pane is through a file it can open, while the clipboard belongs to the device displaying the page. Therefore the browser uploads the image to `POST /api/paste-image`, then types the path written by the server into the pane. Passing only key input cannot work in principle: the CLI's `Ctrl+V` reads the clipboard of the machine where the CLI runs.

The body uses raw bytes instead of base64 JSON. Base64 would add a direct dependency and increase transfer size by one third; changing `conn::read_request` to return the body as `Vec<u8>` instead of `String` does not change the behavior of existing routes. Sending it as a WebSocket binary frame was rejected because it would change the terminal protocol's JSON-only client-to-server contract more substantially.

Files are written to `~/.nightcrow/tmp`, outside the repository. If they were stored inside a repository, each paste would create an untracked file in status, even though a screenshot is not a project change; an in-repository file could be referenced by `@relative-path`.

## Distribution and updates

### Verify the official update binary before recoverable replacement

By default, `nightcrow update` reads only published stable GitHub Releases from `code0xff/nightcrow` and downloads the allowlisted raw binary for the current platform. Rebuilding from source after first removing the existing binary would require a Rust toolchain and jeopardize the installation path during a failed download. Instead, stream the new binary to a sibling temporary file, verify it against the GitHub asset digest, then replace it through the existing self-replacement recovery boundary. If the binary has no digest, trust only `SHA256SUMS` after verifying it with its own API digest. Only explicit `--path` and `--git` options retain the developer `cargo install --locked --force` path.

The HTTP client uses rustls-only `ureq`, matching the command's synchronous execution model and existing decision not to include an async runtime. `reqwest` has broader features and a common async setup that are unnecessary for this single GET flow; the standard library does not provide an HTTPS client. Digests use RustCrypto `sha2`'s core-only SHA-256 rather than an OS command or platform crypto API, so the three platforms share one verification path.

## Plugin trust model

### Process + NDJSON instead of dylib

Rust has no stable plugin ABI, so a dylib would couple compiler/runtime versions and create address-space safety issues. Isolating plugins as child processes and communicating over versioned NDJSON lets the host bound lines/payloads and avoid propagating plugin crashes to panes.

### Shared memory opens SQLite directly without a server

`nightcrow-memory`, which lets agents in panes share notes, uses an MCP helper in each agent to open a project-specific SQLite file directly instead of a host-started plugin process and socket. The host runs only plugins named by a pane or enabled by `watch_on_signal`; using that switch to launch a process that watches no pane would distort its intended purpose. A single-writer process would provide only serialization already provided by SQLite. Without a server there is also no need for a Windows socket transport layer or server process lifetime management. If sending input to another pane becomes necessary, add a host plugin then while keeping the storage format and tools unchanged.

Storage uses `rusqlite` (MIT, `bundled`). Multiple processes need concurrent writes and full-text search (FTS5); files (JSONL/Markdown) would need custom locking and indexes, while embedded key-value stores such as `sled` and `redb` allow only one process to open a file and do not fit a helper-per-process design. `bundled` requires a C compiler but provides the same SQLite and FTS5 across all three platforms. MCP is implemented without an SDK: the helper uses only four stdio JSON-RPC methods, while the official Rust SDK brings in an async runtime.

### Pane opt-in requires token proof and a guard

By default, only panes named by a startup command are exposed to a plugin. Even with `watch_on_signal` enabled, the plugin must present a random `PaneToken` injected only into the pane child. The token alone does not grant access: `Guard` rechecks generation, liveness, launch command, other watchers, and rate budget. Relaunch budgets are counted by token so the same slot stays grouped when it receives a new `PaneId`.

← [Architecture index](architecture.md)
