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

## Bundled `nightcrow-memory`

`nightcrow-memory` gives the coding agents running in one project's panes a shared set of notes and a status board, as tools they call over the [Model Context Protocol](https://modelcontextprotocol.io). An agent in one pane saves a fact; an agent in another pane of the same project can search for it, including after both have exited.

It is not declared under `[[plugin]]` and nightcrow does not start it. Each agent's CLI runs it as a local MCP server, and it finds its project through two variables nightcrow sets on every pane: `NIGHTCROW_PLUGIN_RUNTIME_DIR`, whose last component is the per-project key, and `NIGHTCROW_PANE_TOKEN`. Started anywhere else, the server still answers and every tool call fails with the reason. It never reads or types into a pane.

Build and install it from a checkout:

```bash
cargo build --release -p nightcrow-memory
nightcrow plugin install target/release/nightcrow-memory --name memory
```

Ignore the `[[plugin]]` snippet the install prints. Register the installed executable with each agent instead, with `mcp` as its only argument:

```bash
# Claude Code
claude mcp add --scope user nightcrow-memory -- ~/.nightcrow/plugins/memory mcp
```

```toml
# Codex CLI — ~/.codex/config.toml. Codex gives an MCP server a limited
# environment, so the two pane variables have to be forwarded by name.
[mcp_servers.nightcrow-memory]
command = "/home/you/.nightcrow/plugins/memory"
args = ["mcp"]
env_vars = ["NIGHTCROW_PLUGIN_RUNTIME_DIR", "NIGHTCROW_PANE_TOKEN"]
```

```json
// OpenCode — opencode.json
{
  "mcp": {
    "nightcrow-memory": {
      "type": "local",
      "command": ["/home/you/.nightcrow/plugins/memory", "mcp"],
      "environment": {
        "NIGHTCROW_PLUGIN_RUNTIME_DIR": "{env:NIGHTCROW_PLUGIN_RUNTIME_DIR}",
        "NIGHTCROW_PANE_TOKEN": "{env:NIGHTCROW_PANE_TOKEN}"
      }
    }
  }
}
```

The server has been exercised with the official MCP client SDK; the Codex and OpenCode entries follow those tools' documentation and have not been run against them here.

| Tool | Effect |
| --- | --- |
| `memory_write` | Save one note, with optional tags. |
| `memory_search` | Notes matching any word of the query, best match first. A word also matches with an ending attached. |
| `memory_recent` | The newest notes. |
| `memory_delete` | Remove a note by the id shown beside it. |
| `board_post` | Say what this agent is working on. Replaces its previous post and expires (4 hours by default, 24 at most). |
| `board_read` | What every agent last posted and has not yet expired. |

Every entry is labelled with the client's name and the first six characters of its pane token, such as `claude-code@3fa9c1`. That label tells writers apart and is not verified.

Notes are kept in `~/.nightcrow/memory/<project key>.db`, one SQLite file per project and never inside the repository; set `NIGHTCROW_MEMORY_DIR` to keep them elsewhere. A note or post is at most 4 KiB, a note takes up to 8 tags, a project holds up to 2000 notes, and a read returns at most 50. Text over a limit is refused, not cut, and a full project refuses new notes until some are deleted. `nightcrow-memory export`, run in a pane, prints the project's board and notes as Markdown.

Three things to know before turning it on:

- **A note is another agent's words.** What one agent writes becomes part of what another reads, so text an agent picked up from a web page or a file can travel between panes through a note. Every read is prefixed with a notice that entries are unverified peer notes and not instructions, which lowers this risk and does not remove it.
- **Notes are not private or authenticated.** Any process running as you can read and write the file. The directory is created owner-only on Unix.
- **Agents write what they choose to.** Nothing scans a note for credentials. Review with `export` and remove with `memory_delete`.

## Recovery controls

A pending recovery is shown on the pane tab and in the browser. Use `<prefix> c` or the browser control to cancel it. Typing into the pane also cancels the pending recovery. A cancelled recovery does not relaunch the pane.
