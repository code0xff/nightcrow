# Plugin Host

A plugin is a separate child process responsible for provider-specific detection and recovery. The core does not interpret provider names or output semantics; it provides only generic contracts such as panes, idleness, and relaunch. Panes and terminals continue running if a plugin is absent or fails.

## Process and wire boundary

The host starts one configured plugin child per repository hub and exchanges newline-delimited JSON (NDJSON) over stdin/stdout. Host → plugin messages are `PaneOpened`, `PaneOutput`, `PaneIdle`, `PaneExited`, `PaneClosed`, `UserInput`, and `Shutdown`; plugin → host messages are `SendInput`, `Relaunch`, `Status`, `WatchPane`, `Attention`, and `Log`. Independently deployed plugins are rejected when their `PROTOCOL_VERSION` differs from the host's `PROTOCOL_VERSION = 3`.

Each line is limited to 64 KiB, and pane input from a plugin is limited to 8 KiB. Embedded newlines, malformed JSON, unknown versions, and payloads that exceed bounds are rejected and logged rather than interpreted as commands. The host reader uses a bounded queue, and the host cleans up its child when it exits. On Windows, only pipes are used; no new console is created for the child.

## Trust boundary

`protocol::decode_command` checks only shape and size; `Guard::judge` makes every authorization decision. There is no path to manipulate a PTY or session while bypassing the guard.

- By default, only panes whose startup command names a plugin are opted in. `WatchPane` may expand this set only when `watch_on_signal` is enabled.
- `PaneToken` is generated randomly at pane spawn and injected only into that pane's child environment. `WatchPane` uses the token to find the pane, then rechecks operator permission, whether another watcher exists, and live-process status. The token is an identity/correlation key, not authorization by itself.
- Pane-scoped commands require both a token and `PaneGeneration`. They are rejected if the generation does not match the replacement process. `SendInput` requires a live, idle pane; `Relaunch` requires an exited pane and its original launch command. A plugin cannot choose an arbitrary executable or command.
- Relaunch flags/options absent from `allowed_resume_flags` are rejected. Only a validated single line based on the original command line is run. Input/relaunch approval counts use a per-token window budget that spans relaunches, not a per-pane ID budget.
- Only one watcher is allowed per pane. A plugin's pending requests and outbound events are also bounded, and there is no API for enumerating panes to choose from.

Relaunch does not revive the same `PaneId`; it creates a new ID with an incremented generation. The slot is retained briefly apart from the process and discarded after `PENDING_RELAUNCH_TTL`. A pane without a reproducible launch command, such as a bare shell, is not relaunched and goes directly to the attention/exit path.

## Recovery plugin boundary

`plugins/nightcrow-recovery` is the provider-specific adapter. The bundled recovery plugin identifies Codex CLI and OpenCode from the launch command supplied by the host and interprets each provider's session ID, reset time, and resume arguments only inside the plugin. For Codex it reads an unambiguous session ID and usage-limit reset from rollout JSONL and suggests `codex resume <SESSION_ID>`. For OpenCode it observes `/session/status`, does not intervene during retry, reports `NeedsAttention` when the live process becomes `idle`, and suggests relaunch with `--session <SESSION_ID>` only after the process exits. It does not bypass provider limits or send transcripts or raw payloads outside the host contract.

## Pane helper: shared memory

`plugins/nightcrow-memory` is not a plugin child started by the host; it is a helper launched by the provider CLI inside a pane. It does not use the plugin protocol or appear in `[[plugin]]`. Its only contract with the host is the hub key in the last path component of `NIGHTCROW_PLUGIN_RUNTIME_DIR` and the `NIGHTCROW_PANE_TOKEN` environment variable, both injected at pane spawn. Therefore it does not pass through the guard, watcher, or generation logic, and has no path to read from or send input to a pane.

Each helper acts as a stdio MCP server and opens one SQLite file identified by the hub key. No server process sits in between because SQLite already serializes writers across processes. WAL and busy timeout handle concurrency; opening retries only for a journal-mode transition because SQLite immediately returns `BUSY` there without invoking the busy handler. Data is stored outside the repository (`~/.nightcrow/memory`), and the hub key format is validated before being used in a filename.

The token is not an authentication mechanism here either. Writer labels include only its first 6 characters; the full value is never stored. Items read from the database are untrusted text written by another agent, so every read result includes a notice to that effect. This does not structurally prevent prompt injection from spreading between agents.

## Config reload

The repository hub's worker applies `[[plugin]]` enabled/opt-in settings and the live host list. Only `command`, `args`, and `env` replace a child; `allowed_resume_flags` and `watch_on_signal` take effect from the next guard decision. A plugin already watching a pane remains active until explicitly set to `enabled = false`. If spawning its successor fails, the existing pane hold is discarded to avoid a recovery with no owner. Guard and token budgets are not recreated on reload.

## Recovery surface

The hub does not interpret the meaning of plugin `Status` and `Attention` events; it broadcasts them to all clients. One label is read: entering `needs_attention` is published, like an `Attention` command or a `nightcrow notify` from inside a pane, to the session's `Notices` listeners (`session::notice`), which reach clients that are not attached to that repository. The hub owns only relaunch holds for exited pane slots, but since reports are broadcast once and plugins do not repeat them, the last report is cached per pane in `Shared.recovery` and replayed to clients that attach later. The cache is updated under the same lock as the broadcast, and `cancelled` removes its entry. When a hold ends through cancellation, TTL expiry, successful relaunch, or pane close, the hub sends `Recovery { state: "cancelled" }` to clear a stale deadline. Some paths can leave a report without a hold (a reload drops its watcher and releases the pane, or that pane exits/closes); these send `cancelled` only when a report is cached, while hub teardown clears every remaining report. If a host dies without notifying the hub, its report remains until reload or close; host supervision is an existing limitation. `CancelRecovery` processes a hold only in this order: `pane_closed → forget → retire_slot`.

The TUI shows recovery in the pane tab marker and notice-row chip; the web viewer shows it in the terminal toolbar/pane metadata. Recovery does not add a dedicated row or terminal overlay, so PTY geometry does not change. If there is no deadline, no time is guessed or drawn. Recovery details carry only short host/plugin text, never transcripts or raw payloads.

← [Architecture index](../architecture.md)
