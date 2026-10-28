# Session and Backend

`session/` is the boundary where the daemon owns transport-neutral session state. The attach TUI and web viewer each translate their requests, authentication, and wire protocol into these operations; they do not directly own the catalog, hub, preferences, or PTY size ownership.

## TerminalBackend

```rust
trait TerminalBackend {
    fn create_pane(&mut self, rows: u16, cols: u16, command: Option<&str>) -> Result<()>;
    fn destroy_pane(&mut self, id: PaneId);
    fn send_input(&mut self, id: PaneId, data: &[u8]) -> Result<()>;
    fn resize(&mut self, id: PaneId, rows: u16, cols: u16) -> Result<ResizeOutcome>;
    fn reorder(&mut self, order: &[PaneId]);
    fn claim_size(&mut self);
    fn cancel_recovery(&mut self, pane: PaneId);
    fn drain_events(&mut self) -> Vec<BackendEvent>;
}
```

`PtyBackend` owns local children through `portable-pty` and reader/waiter threads; `HubBackend` sends requests to the daemon hub. On Unix, `portable-pty` successfully calls `setsid()` in spawn's pre-exec hook, establishing the child PID as the session/process-group ID. Attach must establish this boundary before waiting/reaping. This lets it attach to an already-exited but not-yet-reaped child without checking for a live process. Each pane uses its Unix session or Windows Job Object, established at creation, as the termination boundary, so `destroy_pane` and hub shutdown also terminate subprocesses inside it. Plain detach, quit, or browser disconnect does not call the pane-destroy path and therefore leaves processes running. Pane ID/title/resize/reorder are not confirmed as local state immediately; they follow backend events such as `Created`, `Resized`, `Reordered`, and `Exited`. `drain_events` only reports; the owner that receives `Exited` calls `destroy_pane` to reclaim resources. Both backends use the client-side `PaneEmulator` for VT parsing. The pane child environment is set for the emulator that renders the pane, not inherited from the daemon: force `TERM=xterm-256color` and `COLORTERM=truecolor`, and remove `NO_COLOR`. The daemon may start outside a terminal, such as from an agent shell or service manager, whose environment commonly sets `NO_COLOR=1` and `TERM=dumb` for its own children.

Each local PTY buffers at most 256 output events in a synchronous bounded queue, with each event limited to 4096 bytes. When the queue fills, its reader stops reading from the child so PTY backpressure reaches that child; other panes use separate readers and queues. Pane input is handled by a dedicated FIFO writer thread. A successful `send_input` means the entire request was accepted by the asynchronous writer queue. The queue allows up to 4 MiB and 256 messages, including the message currently being written. Requests that exceed the limit, or arrive after writer failure, return an immediate error. Pane teardown closes the output receiver first to wake a blocked reader; the reader continues discarding output bytes until the master returns EOF.

Session limits are 8 PTYs per repository, pane dimensions of 1–500 rows × 1–1100 columns, and 256 KiB reconnect scrollback per pane. Close and resize use dedicated latest-state paths outside the bounded input queue so a full queue cannot lose the final request.

## Shared state

The session shares repository membership/order, active repository, pane set/content/order/title/confirmed size, and accent. Cursor, scroll, focus, fullscreen, search, and the TUI `Workspace` view state are client-local. The viewer shares only sidebar width, `upper_pct`, and per-project last view/maximize in `viewer.json` between browsers; these are separate from the TUI workspace file.

### Catalog transaction

`CatalogMembership` computes a pure union of base config, browser-added paths, hidden paths, and explicit order, with opaque IDs. `CatalogRuntime` reconciles this result, preserves the same `Arc<RepoEntry>` for unchanged paths, and creates status runtime and terminal hub only for new entries. Changes to membership, runtime, and config table are serialized by a catalog facade transaction. Workers for replaced entries are stopped/joined after releasing every catalog lock.

Repository paths are canonicalized at the catalog boundary. Different spellings of the same worktree or a trailing separator do not create duplicate projects. Session open records the canonical path as the active preference; close checks the current focus before recording a successor, so it does not overwrite a concurrent focus change.

### Session watcher

Browser HTTP and attach sockets are separate request paths, so `daemon/watch.rs` observes changes to repository set, active repository, and accent. The watcher rereads the session on a 150 ms tick or after an attach mutation nudge, and broadcasts only when the value differs from the last value sent. It is the only producer that sends repository sets; it also connects the terminal subscription for a newly served repository before broadcasting the set. A daemon that cannot start the watcher does not run.

## PTY size ownership

The width rendered by a PTY child is a session-wide contract with one owner because alternate-screen output cannot be rearranged afterward. The first connection initializes only an unowned session; later connections and reconnects do not take ownership. TUI key press/repeat, paste, mouse down/wheel, and direct web interaction request `claim_size`; resize, redraw, focus, and reconnect do not. `size_owner` sends a decimal-string generation that increments only when ownership changes. A repeated claim from the same viewer resends `owned=true` with the same generation to confirm an explicit refit; a client that missed an intervening ownership change can refit if the generation on a late `owned=true` differs. Legacy frames with no generation are read as unknown. After the owner leaves, a 2-second `RELEASE_GRACE` passes ownership to the most recently active remaining viewer, or to the most recently connected viewer if there is no interaction history. If there are no viewers, the session retains no owner and its last confirmed size.

The TUI resizes panes to the outer window only when it is the owner. A size change does not take ownership away from another client. The server confirms a claim caused by direct input; a same-owner `SizeOwner` response does not force a refit. `<leader> r` is an exceptional recovery action: it requests ownership again, resends current geometry, and redraws the whole screen.

Non-owner resize requests are discarded, and only `Resized` events successfully applied to the PTY are broadcast. The owner separates desired, pending, and confirmed size; if a late confirmation describes an older size, it re-requests the desired size. Resize uses a latest-value queue per connection and pane, separate from the general input queue, so a full queue cannot lose the last width. The ownership generation is recorded when enqueueing and rechecked when applying. Locks are acquired in hub-state → session-ownership order, and the ownership lock is held through PTY application, size recording, and broadcast to serialize claims and resizes. Therefore a delayed resize from owner A cannot apply after A→B→A. Owner verdicts are sent nonblocking through the existing bounded client queue. If that queue is full, a verdict may be dropped; the next input claim or reconnect reports current ownership again.

## Status snapshot

`SnapshotChannel` reads status and watches the filesystem only while it has subscribers. An on-demand `/api/status` request may read once without subscribers. It uses a recursive worktree watcher and a watcher for a separate Git directory (`git worktree`/`separate-git-dir`); if watcher installation fails due to Linux watcher limits or permissions, it falls back to a 1-second timer. A healthy watcher waits at least 1 second between reads and at most 10 seconds to cover missed events. Paths ignored by Git do not trigger a read in the event filter.

On a sleep-to-awake transition it reads once immediately; after awake is disabled it does not publish in-flight stale results. Watcher event backlogs are absorbed at once. It handles both linked-worktree Git directories and canonical-path differences reported by macOS/Windows. Dropping `SnapshotChannel` sends a stop signal and uses bounded `try_timed_join`.

Status payloads are complete latest-state snapshots and can be conflated during runtime fan-out. Terminal bytes, by contrast, are an ordered FIFO stream and are never dropped or conflated. An attach reader drains only a per-repository FIFO prefix, up to 64 messages/256 KiB per tick; a connection inbox is capped at 256 MiB or 4096 messages. A connection that exceeds these bounds is closed and the client reconnects explicitly.

## Terminal replay and reconnect

The hub emulator remembers each pane's current terminal modes and OSC title. On connection it replays a mode prelude and title, then sends a screen snapshot followed by `since` bytes produced after that snapshot. Alternate screen uses the current screen snapshot; normal screen combines ring history, a normal snapshot, and the tail. A snapshot boundary does not split an open escape, multibyte, or synchronized-update sequence; if the boundary is delayed too long, a bounded fallback is used. Neither `screen` nor `since` drops intermediate bytes.

Replay is split into 1 MiB chunks and daemon frame payloads are limited to 4 MiB. A connection receives `Hello`, the selected zoom, each pane's `Created` and replay bytes, then `ReplayComplete`; the completion marker arrives before size-owner verdicts and live broadcasts. Initial replay is limited by the existing client queue depth and reserves one separate slot for the completion marker. If an attach client's terminal inbox overflows, the connection closes rather than dropping some bytes and continuing. After reconnect, the client emulator applies the same byte stream again.

The TUI `HubBackend` marks pane bytes received before `ReplayComplete` as `ReplayOutput`. During this period, PTY query replies generated by the emulator from historical output are suppressed, while title and screen updates still apply. At the completion marker it first settles any open synchronized update and applies its side effects, then lifts suppression. This prevents DSR replies contained in replay from being sent back as input to the running program.

## Config reload

`POST /api/reload` and an attach reload request invoke the same transport-independent operation. The session changes only after all of `config.toml` parses and validates; a missing or invalid file leaves the session unchanged. `[[plugin]]` changes are requested immediately on open repository hubs, and the child is replaced only when `command`/`args`/`env` change. `allowed_resume_flags` and `watch_on_signal` are read from the next decision onward. Changes to `[[startup_command]]` and `[terminal] auto_open` apply only to subsequently created hubs. Web/listener, log, layout/input/tree/mouse settings require a restart.

The reload lock serializes concurrent reloads; the catalog transaction prevents reload and project open from seeing different config tables. A repository whose reload could not be delivered because the hub queue was full is listed as `unreachable`; the reload result is returned only to the requesting client. Plugin reload does not silently revoke opt-in for existing panes or recreate relaunch budgets.

## Worker lifecycle

Workers that send one reply after completion are stopped by dropping the receiver/owner first; they are not joined on hot UI paths. At quiescent points such as drop, repository switch, or reply drain, they are reclaimed with `platform::threading::try_timed_join`. Long-lived `GitLoadWorker`s coalesce each lane to one pending request and stop through a flag/condvar. They maintain process-wide and per-repository Git I/O permits, as well as hard bounds on worker threads/FDs; the `(repository, generation)` guard discards late replies. In-flight libgit2 calls are not forcibly interrupted; handles that exceed the limit are detached.

← [Architecture index](../architecture.md)
