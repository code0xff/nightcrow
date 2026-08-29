# Plugins

A plugin is a separate executable that receives events from selected terminal panes and may request status updates, input, or a relaunch. Plugins are disabled unless explicitly enabled and opted into; ordinary panes are not exposed.

## Install and enable

`nightcrow plugin install` copies an executable to `~/.nightcrow/plugins` and prints a configuration snippet. It does not edit your config or enable the plugin.

```bash
nightcrow plugin install PATH [--name NAME] [--force]
nightcrow plugin list
nightcrow plugin remove NAME
```

Declare and enable the plugin in `~/.nightcrow/config.toml`, then set its name on a `[[startup_command]]` pane. The complete field reference and an example are in [Configuration → `[[plugin]]`](configuration.md#plugin). `args` are passed verbatim and `[plugin.env]` affects only the plugin process. Plugin names must be unique. `allowed_resume_flags` is an allowlist for arguments a plugin may append when relaunching a configured pane; an empty list forbids relaunch arguments.

Set `watch_on_signal = true` to allow a process started inside an unconfigured pane to opt in using its pane token. This is off by default. Such a pane can be monitored and receive plugin input, but cannot be relaunched because nightcrow did not start its command. A plugin never receives a list of panes and cannot address one that has not opted in.

Changing plugin configuration with [config reload](configuration.md#reloading) applies it to open projects. Replacing `command`, `args`, or `env` restarts the plugin; any recovery that was pending in that process is abandoned. Disabling or removing a plugin stops watching its panes but leaves the terminal programs running.

## Bundled `nightcrow-recovery`

Build and install the bundled plugin from a checkout:

```bash
cargo build --release -p nightcrow-recovery
nightcrow plugin install target/release/nightcrow-recovery --name recovery
```

The plugin recognizes Codex CLI and OpenCode. Codex recovery reads the pane's rollout JSONL, requires an unambiguous session id, and relaunches with `codex resume <SESSION_ID>` after the process exits; it never uses `--last`, which could select another pane's session. OpenCode polls `/session/status` and remains hands-off while the provider reports `retry`. When a live process becomes `idle`, recovery reports `NeedsAttention` without interrupting it. If the process exits, the exact session can be relaunched with `--session <SESSION_ID>`.

## Recovery controls

A pending recovery is shown on the pane tab and in the browser. Use `<prefix> c` or the browser control to cancel it. Typing into the pane also cancels the pending recovery. A cancelled recovery does not relaunch the pane.
