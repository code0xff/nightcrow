# Configuration

nightcrow reads `~/.nightcrow/config.toml`. Every field is optional and omitted fields use the defaults below. A first run uses those defaults, then writes a generated web-viewer password to the config unless a password or hash is already configured. Run `nightcrow init` to create the complete commented starter; `nightcrow init --force` replaces an existing file.

## Session and client settings

| Table | Fields and defaults | Valid values / effect |
| --- | --- | --- |
| `[layout]` | `upper_pct = 55`, `file_list_pct = 25`, `tabs = "top"` | The percentages are `1..=99`; TUI panel proportions. `tabs` is `top` for one row across the screen or `left` for a 20-column strip down the body's left edge, one project per row — see [Projects](projects.md) for what the strip shows. |
| `[theme]` | `name = "yellow"` | `yellow`, `cyan`, `green`, `magenta`, or `blue`. Seeds the session accent when no saved accent exists. |
| `[input]` | `leader = "ctrl+f"` | One `ctrl+<ascii-letter>` chord. `ctrl+i` and `ctrl+m` are rejected because terminals report them as Tab and Enter. |
| `[mouse]` | `enabled = true` | Captures clicks and wheel events for the TUI; `false` gives selection and mouse handling back to the outer terminal. |
| `[terminal]` | `auto_open = false` | With no startup commands, `true` opens one shell per project automatically; `false` waits for `<prefix> t`. |
| `[agent_indicator]` | `enabled = true`, `hot_window_secs = 15`, `auto_follow = false` | Hot window is `3..=3600` seconds. `auto_follow` selects the freshest recently changed file after 2 seconds of inactivity. |
| `[tree]` | `respect_gitignore = true`, `max_depth = 64`, `live_watch = true` | `max_depth` is `1..=1024`; `live_watch = false` refreshes the tree on entry instead of watching expanded directories. |
| `[shell]` | `program` omitted; `command_args` platform default | Unix uses `$SHELL` or `/bin/sh` with `[-lc]`; Windows uses `%ComSpec%` or `cmd.exe` with `[/C]`. The command is always the final single argument; interpolation such as `"{}"` is not supported. |

The web viewer has its own panel proportions in `~/.nightcrow/viewer.json` and keeps its sidebar width in the browser; `[layout]` controls the TUI only. Shared files and ownership are described in [Session state](session-state.md).

## `[web_viewer]`

The viewer is always part of a session. Defaults are `bind = "127.0.0.1"`, `port = 8091`, and `session_ttl_hours = 24`.

```toml
[web_viewer]
bind = "127.0.0.1"
port = 8091
# password = "..."
# hashed_password = "$argon2id$v=19$..."
session_ttl_hours = 24
```

`bind` must be an IP address and `port` must be non-zero. `session_ttl_hours` accepts `0..=87600` hours; `0` means sessions do not expire on the server, while browser cookies still have a 400-day maximum. If neither credential is set, startup generates a random password, saves it to this file, and prints it once. `hashed_password` is an Argon2 PHC string and takes precedence over `password`. Login attempts are rate-limited, logout revokes the server-side token, and persisted tokens live in `~/.nightcrow/sessions`.

The command-line options `--bind ADDRESS` and `--port PORT` override these values for one daemon run. The listener uses plain HTTP, so remote access requires an SSH tunnel or TLS reverse proxy; see [Web viewer → Access and security](web-viewer.md#access-and-security).

## `[log]`

```toml
[log]
enabled = true
dir = ".nightcrow/logs"
rotation = "daily"
max_size_mb = 10
max_days = 7
level = "info"
prompt_log = false
commit_log_page_size = 100
commit_log_prefetch_threshold = 25
```

Relative `dir` values are under the user's home/state directory. `rotation` is `daily`, `hourly`, or `size`; `max_size_mb` is `1..=10000` and is used for `size`; `max_days = 0` keeps logs forever, otherwise it is at most 3650 days. `level` is `error`, `warn`, `info`, `debug`, or `trace`. `prompt_log` records terminal prompt input line by line and is off by default. `commit_log_page_size` is `50..=500`; the prefetch threshold is `1..=page_size`.

## `[[startup_command]]`

Each entry opens one terminal pane per project and runs `command` through the configured shell. `name` is an optional tab label. `plugin` optionally names a declared plugin for that pane.

```toml
[[startup_command]]
name = "Codex"
command = "codex"
plugin = "recovery"

[[startup_command]]
command = "cargo test --watch"
```

Configured entries and repeated CLI `--exec COMMAND` values share an 8-pane startup limit, in config-first order. `command` cannot be empty. A project with no startup entries starts with no panes by default; set `[terminal] auto_open = true` to restore one automatic shell. Each project may hold up to 8 panes total.

## `[[plugin]]`

Each plugin entry requires a unique `name` and executable `command`; `args` and `[plugin.env]` are optional and apply to the plugin process only.

```toml
[[plugin]]
name = "recovery"
command = "nightcrow-recovery"
args = []
enabled = false
watch_on_signal = false
allowed_resume_flags = []

[plugin.env]
PLUGIN_LOG = "info"
```

Plugins are off unless `enabled = true`. A plugin normally receives events only from panes whose `[[startup_command]]` sets `plugin =` to its name. `watch_on_signal = true` also permits a process inside an otherwise unconfigured pane to opt in with its pane token; such a pane can be monitored and receive input but cannot be relaunched. `allowed_resume_flags` is an explicit allowlist for flags/subcommands a plugin may append when relaunching a configured pane; leave it empty to forbid relaunch arguments. At most 8 plugin entries are allowed.

See [Plugins](plugins.md) for installation and the bundled recovery plugin.

## Reloading

Use `<prefix> u` in the TUI or the reload control in the browser. nightcrow parses and validates the whole file before applying anything; a missing, malformed, or invalid file leaves the running session unchanged.

- `[[plugin]]` is re-applied immediately to open projects. Changing a plugin's executable, arguments, or environment restarts that plugin and can abandon a pending recovery.
- `[[startup_command]]` and `[terminal] auto_open` apply to projects opened after the reload. Existing project panes keep running; CLI `--exec` panes remain part of the merged startup list.
- All other settings require a daemon restart. A TUI reads its client settings when it attaches, while the running daemon keeps its listener and server settings until restart.

Restarting a session stops its terminal programs. Use [Getting started](getting-started.md#detach-and-stop) for the shutdown procedure.
