# Getting started

## Install

Install straight from the repository (the built viewer bundle is committed, so
this needs no Node toolchain):

```bash
cargo install --git https://github.com/code0xff/nightcrow --locked
```

Or build a local checkout:

```bash
cargo install --path . --locked
```

Once published to crates.io this will also work:

```bash
cargo install nightcrow --locked
```

Requires Rust 1.85+ (edition 2024). `--locked` builds against the committed
`Cargo.lock` for a reproducible install.

## Running a session

nightcrow runs as a **session**: one process holds the repositories and the
terminals, and you reach it from a terminal (`nightcrow attach`) or a browser.
Closing a client leaves the session running.

```bash
# Start the session. Runs in the foreground until you stop it (Ctrl-C).
# It reopens the repositories from last time — nothing, on a first run.
nightcrow

# ...or run it in the background and get your shell back.
nightcrow -d

# From another terminal: bring up the TUI on that session.
nightcrow attach

# Launch terminal panes running commands at startup (repeatable)
nightcrow --exec "claude" --exec "codex"
```

The session prints the address of its browser view (`http://127.0.0.1:8091/`
by default) and the socket an attaching terminal uses. Both show the same
repositories; open one with `<prefix> o` in the TUI or the folder picker in the
browser, and it appears in the other. There is no flag for opening a
repository — a session several clients share has one sensible place to do it,
and that is inside.

## Detaching, disconnects, and shutdown

Leaving the TUI (`<prefix> q`) detaches: the session, and everything running in
its terminals, keeps going. Stopping the session is stopping the process you
started it in — or `kill`ing it, if you used `-d`, in which case its output is
in `~/.nightcrow/daemon.out`. Under a service manager, start it *without* `-d`:
backgrounding is what the manager does itself.

If the connection to the session ends under it — the session was stopped, or it
dropped a client that fell too far behind — the TUI leaves and says so, with a
non-zero status. What it had selected and scrolled is written back either way, so
reattaching returns to it. There is no automatic reconnect: reattach when the
session is up.

## Startup panes

Startup panes belong to a project, not to the process: each project you open
gets its own set. So `nightcrow --exec claude` with no repositories starts
`claude` in the first project opened, not before there is one to open it in.

`--exec` panes open after any `[[startup_command]]` panes from the
[config file](configuration.md#startup_command); the two sources share a
combined cap of 8 panes — the same count the `<prefix> 3`–`9`,`0` jump keys
address, so every startup pane is reachable by a direct key. (`<prefix> 1`/`2`
map to the file list and diff viewer.) Panes opened later with `<prefix> t` are
not capped; any past the eighth are reached by focus cycling (`Shift+←/→`).

## Where to go next

- [Projects](projects.md) — opening repositories as tabs
- [Views](views.md) — what each panel shows
- [Keyboard and mouse](keybindings.md) — the full binding reference
- [Configuration](configuration.md) — `nightcrow init` and every setting
