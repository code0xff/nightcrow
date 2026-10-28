# Plugin crates

This guide applies to independently built plugin crates under `plugins/`. Follow the [root `AGENTS.md`](../AGENTS.md) for repository-wide rules, and use [Plugins](../docs/plugins.md) and [Plugin Host](../docs/architecture/plugin-host.md) as the sources of truth for plugin contracts.

`nightcrow-memory` is an exception: it is an MCP helper started by a provider inside a pane, not a plugin run by the host. The host-boundary rules below about the plugin protocol do not apply to it, but it must still keep repository instances separate and must not use pane tokens as authentication. See [Plugin Host](../docs/architecture/plugin-host.md#pane-helper-shared-memory) for its contract.

## Host boundary

- A plugin is a separate process, not a library inside the host's address space. Do not depend on host-internal modules or the Rust ABI; communicate only through stdin/stdout NDJSON and an explicit protocol version. If the wire format changes incompatibly, update both sides of the contract together and do not guess at recovery from a version mismatch.
- Plugin instances run per repository, but are not global singletons. Use the runtime directory injected by the host so plugins and pane helpers find the same socket; do not use cwd or a fixed global socket path that could mix repository instances.
- A pane token is a correlation key, not an authentication mechanism. Do not enumerate panes or infer targets from cwd. Let the host guard decide whether to adopt a helper-provided token through `WatchPane` and authorize every pane-scoped command (input, relaunch, status, and attention). Commands carrying a generation apply only to the current spawn.
- Input and relaunch plans returned by an adapter are proposals. Do not bypass provider limits or add permission arguments arbitrarily; rely on the user's configured allowlist and the host's checks for liveness, idle state, generation, and launch command. A pane where the user started a provider manually may be watched or sent input, but must not be relaunched.

## Failure isolation and provider boundary

- Keep provider-specific detection, session identifiers, and resume arguments inside the plugin. If an exact reset time is available, handle it with one bounded wait; otherwise fall back to bounded backoff, and do not intervene while the provider is retrying on its own.
- Bundled recovery identifies the provider from the launch command supplied by the host. Keep `watch_on_signal` and `WatchPane` as public host contracts used by external plugins; do not assume bundled recovery has a separate token-adoption path.
