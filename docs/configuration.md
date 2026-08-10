# Configuration

Config file: `~/.nightcrow/config.toml` (all fields optional, defaults shown).
nightcrow runs on built-in defaults when the file is absent and never creates it
on its own. To get a starter file, run:

```bash
nightcrow init            # writes a commented ~/.nightcrow/config.toml
nightcrow init --force    # overwrite an existing file
```

`init` leaves an existing config untouched unless `--force` is passed.

## `[layout]`

```toml
[layout]
upper_pct = 55       # vertical % for the diff panel (1–99) — the TUI's own; the
                     # viewer keeps a separate dragged value in viewer.json
file_list_pct = 25   # horizontal % of upper panel for the file list (1–99)
```

## `[theme]`

```toml
[theme]
name = "yellow"      # accent a session starts with, before anyone picks one:
                     # "yellow" | "cyan" | "green" | "magenta" | "blue"
```

## `[input]`

```toml
[input]
leader = "ctrl+f"    # leader (prefix) chord for app commands; tmux-style.
                     # Allowed: "ctrl+<letter>". Reserved keys (F1..F10,
                     # Shift+arrows, Shift+PgUp/PgDn) cannot be the leader.
```

## `[mouse]`

```toml
[mouse]
enabled = true       # capture the mouse: click to focus/forward, wheel scrolls
                     # the pane under the pointer; select text with the
                     # terminal's bypass modifier + drag (Shift in xterm-family,
                     # Option in iTerm2, Fn/Option in macOS Terminal.app).
                     # false = plain-drag selection, no click forwarding.
```

## `[web_viewer]`

```toml
[web_viewer]
bind = "127.0.0.1"   # loopback only by default; plain HTTP, so tunnel/proxy for remote
port = 8091
# password = "..."         # auto-generated + saved here on first launch if unset
# hashed_password = "..."  # Argon2 PHC string; takes precedence over `password`
session_ttl_hours = 24     # how long a browser login lasts (0 = never expires)
```

See [Web viewer → Configuration and access](web-viewer.md#configuration-and-access).

## `[log]`

```toml
[log]
enabled = true
dir = ".nightcrow/logs"   # relative paths resolve under the home directory
rotation = "daily"        # "daily" | "hourly" | "size"
max_size_mb = 10          # used when rotation = "size"
max_days = 7              # delete logs older than N days (0 = keep forever)
level = "info"            # "error" | "warn" | "info" | "debug" | "trace"
prompt_log = false        # record terminal prompt input line by line
commit_log_page_size = 100        # commits fetched per commit-log page
commit_log_prefetch_threshold = 25 # start the next-page fetch when the selection is within
                                  # this many rows of the loaded tail (1..=page_size)
```

## `[agent_indicator]`

```toml
[agent_indicator]
enabled = true            # color recently-touched files in the file list
hot_window_secs = 15      # seconds within which a file stays hot (3–3600)
auto_follow = false       # jump selection to the freshest hot file when idle
```

See [Session state → Recent-activity focus indicator](session-state.md#recent-activity-focus-indicator).

## `[tree]`

```toml
# Read-only directory-tree navigator (enter with <prefix> b).
[tree]
respect_gitignore = true  # hide .gitignore-matched paths (target/, node_modules/, …)
max_depth = 64            # deepest directory level the tree will expand into (1..=1024)
live_watch = true         # watch expanded dirs and refresh the tree live; set false
                          # to refresh only on tree entry (large trees / odd filesystems)
```

## `[shell]`

The shell every terminal pane is spawned with. When the whole section is absent,
the platform default is used:

| Platform | `program`                     | `command_args` |
|----------|-------------------------------|----------------|
| Unix     | `$SHELL` env var or `/bin/sh` | `["-lc"]`      |
| Windows  | `%ComSpec%` or `cmd.exe`      | `["/C"]`       |

`command_args` is the flag list placed *after* the shell name. The command text
is always the last single argv item, so the shell — not us — handles its
quoting/word-splitting. Interpolation like `["-c", "{}"]` is not supported: that
would break the contract that the shell owns quoting.

```toml
[shell]
# program = "C:\\Program Files\\Git\\bin\\bash.exe"   # optional; platform default when omitted
# command_args = ["-lc"]                               # optional; platform default when omitted
```

## `[[startup_command]]`

Each entry opens its own terminal pane at launch and runs `command` immediately
(via the configured shell). Up to 8 entries combined with CLI `--exec` — 8
matches the `<prefix> 3`–`9`,`0` jump keys, so every startup pane is reachable by
a direct key. This caps only the startup batch; open more anytime with
`<prefix> t`. With no entries, nightcrow opens a single empty shell.

```toml
[[startup_command]]
name = "Claude"           # optional tab label; falls back to the command text
command = "claude"        # required; must not be empty
plugin = "recovery"       # optional; names the [[plugin]] allowed to act on this
                          # pane. Omitted — the default — means no plugin sees
                          # it unless that plugin sets watch_on_signal.

[[startup_command]]
command = "cargo test --watch"
```

## `[[plugin]]`

External plugin processes — see [Plugins](plugins.md). Up to 8 entries, names
unique. Nothing runs unless an entry exists **and** `enabled = true` **and**
either a pane opted in or `watch_on_signal` is set.

```toml
[[plugin]]
name = "recovery"                 # the name panes opt in with
command = "nightcrow-recovery"    # found on PATH or in ~/.nightcrow/plugins
args = []                         # passed to the plugin verbatim
enabled = false                   # off by default
watch_on_signal = false           # off by default; when true, a pane no
                                  # [[startup_command]] named is also handed over
                                  # once something inside it quotes that pane's
                                  # token to this plugin. Such a pane is never
                                  # relaunched, only typed into while it lives.
allowed_resume_flags = []         # flags/subcommands the plugin may append to
                                  # re-open a session; empty refuses arguments

[plugin.env]                      # plugin process only, never terminal panes
NIGHTCROW_RECOVERY_LOG = "info"
```

## Reloading the config

Editing `config.toml` normally means restarting the session — which kills every
pane, including whatever an agent CLI was in the middle of. Two of the tables can
be re-read instead, without stopping anything:

- **In the TUI**: `<prefix> u`. The result appears on the notice row.
- **In the browser**: the ⟳ button in the header, next to sign out. It reloads
  the *config*, not the page — nothing on screen changes, and the result comes
  back as a toast.

| Table | When it takes effect |
| --- | --- |
| `[[plugin]]` | **Immediately, in every open project.** Newly enabled plugins start and are handed the panes that opted into them; disabled or removed ones stop and their panes carry on unwatched. A plugin whose `command`, `args` or `env` changed gets a new process; changing only `allowed_resume_flags` or `watch_on_signal` leaves the running one alone, so a plugin part-way through a long wait is not disturbed. A replacement that will not start (a command that is not there) leaves its panes unwatched too, exactly as removing it would |
| `[[startup_command]]` | **On the next project you open.** A project that is already open keeps the panes it started with — those are live processes, and no file edit replaces them |
| Everything else | Needs a restart: `[web_viewer]` (the listener is already bound), `[log]`, and the client-owned `[layout]`, `[input]`, `[tree]`, `[mouse]` sections, which each TUI reads when it attaches |

Notes:

- **Nothing half-applies.** The whole file is parsed and validated first, so a
  typo anywhere leaves the session exactly as it was, and the message names the
  key that was wrong.
- **A missing file is refused** rather than read as "nothing is configured" —
  otherwise deleting the file and reloading would be a quiet way to stop every
  plugin.
- Panes opened with `--exec` are kept: they are not in the file, so a reload
  merges them back where a restart would have put them.
- Disabling a plugin and enabling it again lands where enabling it the first time
  would — the pane's opt-in survives, so `enabled` means the same thing whichever
  way it was last flipped.
- **Restarting a plugin discards whatever it was in the middle of.** A plugin's
  state lives in its process, so replacing that process loses it — for
  `nightcrow-recovery` a pane parked on a quota reset hours away simply stops
  being watched, and nothing will resume it. The plugin logs how many panes it
  abandoned on the way out. This only happens when you change *that plugin's* own
  `command`, `args` or `env`; every other edit leaves a waiting one running.
- A pane whose process had already exited and whose slot was being held for a
  relaunch gives that slot up when its plugin is stopped or replaced. The
  successor is never handed the pane's token, so nothing could honour the hold;
  the countdown ends instead of running out its window.
- If the result says **`(1 was too busy to be told)`**, that project kept the
  plugins it had. Its terminals were too far behind to take the request, and
  waiting on one project would have held up every other. Nothing else about the
  reload is affected — reload again once it has caught up. The server log names
  the project.

Design notes: [Architecture → Session](architecture/session.md#config-reload-webviewerreloadrs).
