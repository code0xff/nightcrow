# Plugins

nightcrow itself knows nothing about the CLIs you run in its panes — an agent
and a person get the same PTY. Behaviour that *does* need to know a particular
tool lives in a plugin: a separate executable that nightcrow launches and talks
to over a pipe.

Plugins are off unless you turn one on, and one only ever sees a pane you handed
it by name — or, if you also set `watch_on_signal`, a pane that something running
inside it spoke to the plugin from. A plugin is never given a list of your panes
either way.

## The bundled plugin: `nightcrow-recovery`

Two jobs. It marks a project tab when a pane's agent finishes a turn — Claude
Code, via the `Stop` hook below. And when a watched pane's CLI hits its usage
limit, it waits for the reset time the provider reported and then re-opens that
exact session. It only waits — it does
not bypass, raise, or work around any provider limit, and it sends nothing while
a limit is in effect. Claude Code, Codex CLI, and OpenCode are supported;
OpenCode is only ever observed, never interrupted, because it retries on its own.

```bash
cargo build --release -p nightcrow-recovery
nightcrow plugin install target/release/nightcrow-recovery --name recovery
nightcrow plugin list        # what is installed, and how config refers to it
nightcrow plugin remove recovery
```

`install` prints the exact `[[plugin]]` block to paste, using whatever `--name`
you chose — that name is what a pane opts in with, so keep the two in step.

## Enabling one

Installing only puts the binary in `~/.nightcrow/plugins`. It stays inert until
you edit `~/.nightcrow/config.toml` yourself — enabling something that can type
into a terminal should be a change you read before it takes effect:

```toml
[[plugin]]
name = "recovery"
command = "nightcrow-recovery"
enabled = true
# Resume flags or subcommands the plugin may append. Empty by default, which
# refuses every relaunch with arguments. nightcrow cannot know what a CLI's
# control tokens mean, so it will not add one you did not list — that is what
# keeps a plugin from changing how a CLI asks for your approval.
allowed_resume_flags = ["--resume", "resume", "--session"]

[[startup_command]]
name = "Claude"
command = "claude"
plugin = "recovery"      # without this line, no plugin sees this pane unless
                         # watch_on_signal is set (see below)
```

## Panes you opened by hand

That covers the panes you configured. For the pane you did not — you opened a
shell with `<prefix> t` and typed `claude` into it yourself — add
`watch_on_signal = true` to the `[[plugin]]` block.

nightcrow puts a random token in each pane's environment and nowhere else, so the
CLI's own hook can quote it back and the plugin can ask for "the pane this token
names"; a plain shell never speaks to a plugin, so your shells stay untouched. It
is off by default. Such a pane can be waited for and typed into but never
relaunched — nightcrow launched no command in it, so there is nothing to put back.

## Claude Code hooks

For Claude Code, let the plugin install its hook and statusline entries so it can
read the exact session id and reset time instead of guessing from what is printed
on screen. With a reset time it waits exactly once; without one it falls back to
retrying on a backoff, which can give up. It merges into your existing
`~/.claude/settings.json` and backs it up first:

```bash
nightcrow-recovery install-hooks
nightcrow-recovery uninstall-hooks    # removes only what it added
```

Claude Code's `statusLine` holds one command, so installing does replace yours —
but it is then run from the plugin's own statusline with the same input, and what
it prints is what you see. `uninstall-hooks` puts it back.

Installing also adds a `Stop` hook, which fires as every turn ends and marks that
pane's project tab (see [Projects](projects.md)). This exists because the marker
is otherwise inferred from what crosses the PTY — a terminal bell, or a burst of
title changes — and Claude Code reports a finished turn through desktop
notifications instead, which cross neither. The hook says so directly, so the
marker no longer depends on how long the turn was or how often the title moved.
It carries no payload: that the turn ended is the whole message.

## Cancelling a pending recovery

A pane that is waiting shows its state and deadline on its tab. Cancel it with
`<prefix> c` (see [Leader commands](keybindings.md#leader-commands)), or from the
web viewer; typing into the pane yourself also cancels it.

Design and trust boundary:
[Architecture → Plugin host](architecture/plugin-host.md).
