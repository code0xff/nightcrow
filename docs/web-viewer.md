# Web viewer

The web viewer is always served by a session. It shows the same repositories, project tabs, terminal panes, and session accent as the TUI, at the URL printed when the daemon starts. The browser is another client of the session, not a separate copy.

## Projects and files

The header's project control opens an existing server-side directory, closes a project, or reorders tabs. The same picker can clone a remote repository into the selected directory. Cloning runs `git` on the server and uses that machine's SSH agent or credential helper.

Only `https://`, `http://`, `ssh://`, `git+ssh://`, and scp-style `user@host:path` remotes are accepted. Local paths, `file://`, `git://`, and `ext::` are refused. One clone runs at a time; it continues on the server if the page is closed or reloaded, and the page can resume polling it. A destination with an existing name is rejected.

Each project exposes `status`, `log`, and `tree` views, a diff/file content pane, and the same interactive terminal session as an attached TUI. The [Views](views.md) and [Keyboard and mouse](keybindings.md) guides describe the shared Git and input behavior. The viewer binds the same commands to keys a browser can actually receive rather than to the TUI's physical keys; [Keyboard and mouse → Web viewer](keybindings.md#web-viewer) records which commands the browser keeps unchanged, reinterprets, or leaves unbound, and the in-app shortcut sheet lists them with their keys and marks the ones unavailable on the current screen. On wider screens a one-line hint bar under the footer prints the leader and its follow-up keys the way the TUI does. Browser view state (last tab/file, tree expansion, and maximized panel) is stored separately from TUI view state.

Markdown files render as formatted documents with highlighted fenced code. `.html` and `.htm` files can render in a sandbox that allows inline scripts but blocks cookies, session access, network connections, and external assets; use the raw-source toggle for inspection. The rendered page is a preview of a self-contained file, not a general website.

## Layout and terminals

Drag the sidebar and upper-panel dividers to resize them; double-click a divider to reset it. The browser's sidebar width and upper-panel split are stored in `~/.nightcrow/viewer.json` and shared with other browser clients. They are independent of the TUI's `[layout]` values. The header swatch cycles the session accent and is shared with attached TUIs.

The terminal toolbar can add a pane, show panes as a grid or tabs, maximize the terminal panel, claim sizing for this screen, and show the on-screen key bar. A project has up to 8 panes. Pane order and zoom are shared while the session runs; they are not restored after the session ends. A PTY has one size, so the client that most recently claims sizing determines the grid rendered by every client.

On phones and other narrow layouts, the bottom navigation switches among `Repo`, `Content`, and `Terminal`. Touch-dragging a terminal scrolls it; the key bar supplies Escape, Tab, arrows, and control keys when a soft keyboard cannot. A soft keyboard opening does not resize the panes: the PTY keeps its grid and the pane shows the bottom of it, so a full-screen program is not made to repaint from the top every time the keyboard comes and goes. Layout changes made while the keyboard is up are applied once it closes. Its `Ctrl` button is a latch for the next typed character. The keyboard-bar preference is stored in the browser, so it can be changed from the terminal toolbar. The shortcut leader key is stored in the browser too, and can be rebound or switched off from the shortcut sheet.

Terminal programs may write to the clipboard through OSC 52; the text reaches the browser device viewing the pane. A program requesting clipboard contents is not answered. If the browser requires a user gesture to write, the viewer shows a Copy action.

## Access and security

Configure the listener and credential in [`[web_viewer]`](configuration.md#web_viewer). The default is `127.0.0.1:8091` over plain HTTP. An authenticated viewer grants repository browsing and interactive shell access. For remote use, do not expose the port directly: tunnel it with SSH or put it behind a TLS reverse proxy.

If no password or `hashed_password` is configured, the daemon generates a random password, saves it to `~/.nightcrow/config.toml`, and prints it once at startup. `hashed_password` accepts an Argon2 PHC string and takes precedence over `password`. Login attempts are rate-limited and issue an HTTP-only, SameSite cookie; logout revokes the server-side token. Tokens are persisted in `~/.nightcrow/sessions` and use the configured `session_ttl_hours` (`24` by default, `0` for no server-side expiry). See [Configuration](configuration.md#web_viewer) for limits and reload behavior.

The viewer checks the request host and origin before serving repository data. Repository/path errors are redacted where exposing server paths would be unsafe; clone failures may include actionable Git output. The HTML preview is sandboxed and cannot use the viewer's session or connect back to it.

## Frontend development

The React/Vite source is in `viewer-ui/`; the committed `viewer-ui/dist/` bundle is embedded in release builds. Install Node.js 22 dependencies with `npm --prefix viewer-ui ci`, run `npm --prefix viewer-ui run dev` for a local frontend, and use the verification commands in [Getting started → Building and testing](getting-started.md#building-and-testing).
