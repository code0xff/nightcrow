# Getting started

## Install

Install the released source directly from GitHub:

```bash
cargo install --git https://github.com/code0xff/nightcrow --locked
```

For a checkout you are developing locally:

```bash
cargo install --path . --locked
```

Rust 1.85 or newer is required. `--locked` uses the repository's committed `Cargo.lock`. The browser bundle is committed and embedded in the binary, so these commands do not require Node.js.

Create a commented configuration starter when needed:

```bash
nightcrow init
```

An existing `~/.nightcrow/config.toml` is preserved; use `nightcrow init --force` only when you intend to replace it. See [Configuration](configuration.md) for the fields and defaults.

## Run a session

```bash
# Start in the background if needed, then attach the TUI.
nightcrow attach

# Start in the foreground (Ctrl-C stops it).
nightcrow

# Start in the background and return to the shell.
nightcrow -d
```

There is one session per running daemon. `nightcrow attach` reuses an existing session and starts one in the background when none is available. The daemon owns the open repositories and terminal programs; clients only attach to it. Startup prints the browser URL and the attach-socket path. A background daemon writes its output to `~/.nightcrow/daemon.out`.

Use `--exec COMMAND` once per startup pane when starting a daemon. Configured `[[startup_command]]` entries run first, followed by these CLI commands. The combined startup list is limited to 8 panes per project; each project gets its own list. With no startup commands, a project starts with one shell. All terminal panes in a project share an 8-pane limit; later panes are opened with `<prefix> t` until that limit is reached.

The browser and TUI share repositories, terminals, project order, active project, and accent. The TUI's leader is `Ctrl+F` by default; see [Keyboard and mouse](keybindings.md) for all controls.

## Detach and stop

`<prefix> q` leaves the TUI while the session and its panes continue running. Reattach with `nightcrow attach`. Stop the daemon and its terminal programs with:

```bash
nightcrow stop
```

If a client loses its connection, reattach after confirming that the daemon is still running; there is no automatic reconnect. `nightcrow stop --socket PATH` targets a non-default daemon socket.

## Update

```bash
nightcrow update
```

By default this reinstalls from the upstream repository. Use `--path DIR` for a local checkout or `--git URL` for another Git repository. The command requires Rust and runs a locked, forced `cargo install`. Restart the session after updating so the daemon and its panes use the new binary. On Windows, `update` moves the installed executable aside before replacing it; rerunning plain `cargo install` while a session is running can fail because Windows locks the executable.

## Building and testing

The Rust verification gates are:

```bash
cargo fmt --all --check
cargo build
cargo test
cargo clippy --all-targets --all-features -- -D warnings
```

Enable this checkout's hooks once with `git config core.hooksPath .githooks`. The `pre-commit` hook runs the format check; `pre-push` runs the CI-equivalent gates for the changes in the push range. See [commit rules](../.agents/rules/commits.md) for commit-specific policy.

The viewer source requires Node.js 22 and installed dependencies:

```bash
npm --prefix viewer-ui ci
npm --prefix viewer-ui test
npm --prefix viewer-ui run build
```

The bundle in `viewer-ui/dist/` is committed. A viewer change is complete only when the build succeeds and the generated `dist` diff is included when it changes. On Windows, run the Unix verification gate with:

```bash
docker compose run --rm unix-gate
```

The Docker PTY test can be timing-sensitive under load; rerun the failing test outside the container before treating that failure as a code regression.
