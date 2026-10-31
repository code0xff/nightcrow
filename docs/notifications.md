# Notifications

A terminal pane can ask for its person. The attached TUI marks that project's tab, an open web viewer shows the message as a toast, and browsers that turned on notifications receive a push notification even when closed.

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

## Web Push

When the daemon's web viewer is running, each notice is also sent as a Web Push notification to every browser that turned notifications on. The browser does not have to be open. A notification shows the project name, then the pane's title and the message. Several notices from one pane within 10 seconds produce one notification, and a newer one replaces the older on the device. Tapping it opens the viewer on that project with that terminal selected.

### Turning it on

Open the viewer's settings menu and switch **Notifications** on, then allow notifications when the browser asks. Each browser is switched separately. **Send a test notification** checks the whole path from the daemon through the browser's push service to the device.

Browsers offer Web Push only on a secure page: `https://`, or `http://localhost` on the machine running the daemon. A phone reaching the daemon over a plain `http://` address cannot turn it on; the menu says so. With Tailscale, serve the viewer over HTTPS from the daemon's machine:

```bash
tailscale serve --bg 8091
```

Tailscale forwards the browser's host name, which a viewer bound to loopback (the default) refuses unless it is listed in [`allowed_hosts`](configuration.md#web_viewer):

```toml
[web_viewer]
allowed_hosts = ["<machine>.<tailnet>.ts.net"]
```

Restart the daemon after adding it, then open `https://<machine>.<tailnet>.ts.net` on the phone. Tailscale provisions and renews the certificate. Sign in again on the new address, since a login belongs to one address.

On iPhone and iPad, Web Push works only in a page added to the Home Screen (iOS 16.4 or later): open the viewer in Safari, choose **Share → Add to Home Screen**, open it from that icon, and switch notifications on there.

### When notifications do not arrive

- The switch is missing and the menu says notifications are blocked: the permission prompt was declined. Browsers do not ask twice; allow notifications for the site in the browser's site settings.
- Notifications arrive late on Android: battery optimisation can hold the browser's push connection. Exempt the browser from it.
- Notifications stop after clearing site data or reinstalling the browser: that ends the subscription. The daemon removes it the next time the push service reports it gone, and the menu shows notifications as off again.

The daemon keeps its push key and the list of subscribed browsers in `~/.nightcrow/push/`. Deleting the directory turns notifications off for every browser.
