# Notifications

A terminal pane can ask for its person. The attached TUI marks that project's tab, and an open web viewer shows the message as a toast.

## `nightcrow notify`

Run it inside a nightcrow terminal pane:

```bash
nightcrow notify [MESSAGE...] [--socket PATH]
```

The words after `notify` become one message, trimmed and cut to 200 characters; with none, the notice carries no text. The pane is identified by `NIGHTCROW_PANE_TOKEN`, which nightcrow puts in every pane's environment and which the pane's child processes inherit. Outside a pane the variable is unset and the command fails. The daemon refuses a token that names no live pane in any open repository, and the command then exits non-zero.

Chain it after a long command, or call it from an agent's completion hook. For Claude Code, a `Stop` hook in `.claude/settings.json`:

```json
{
  "hooks": {
    "Stop": [{ "hooks": [{ "type": "command", "command": "nightcrow notify Claude finished" }] }]
  }
}
```

For Codex CLI, `notify` in `~/.codex/config.toml` runs a program when a turn completes. Codex appends a JSON description of the turn as a final argument, so wrap the command in a shell that ignores it; otherwise the JSON becomes part of the message:

```toml
notify = ["sh", "-c", "nightcrow notify Codex finished", "codex-notify"]
```

Both hooks run inside the pane's process tree, so they inherit its token.

## Plugin notices

A plugin raises the same notice with its `attention` command. The bundled recovery plugin does not send that command; instead, when it reports a pane as `needs_attention`, the hub treats entering that state as a notice and uses the report's `detail` line as the message. A repeated `needs_attention` report for the same pane is not announced again until the pane leaves that state.
