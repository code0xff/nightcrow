# Web viewer

A browser surface that renders the same git data as a native web page —
selectable text, real scrolling, clickable paths, and a layout that adapts to a
phone. It also serves the session's terminals, the same panes an attached TUI
sees.

It is always on — it is one of the session's two faces, not an add-on.

## Projects in the browser

The served repositories appear as project tabs in the header — `+ open` browses
the server machine's folders to add one, `×` closes it, and dragging a tab
reorders them.

The same dialog **clones a git URL** into the folder it is showing: paste
`https://…` or `git@host:path`, and the repository opens as a tab when the clone
finishes. Cloning runs `git` on the server, so it uses that machine's credentials
— an SSH agent, a credential helper — and a private remote works exactly as it
would in a shell there. Local paths and git's `ext::` transport are refused. A
clone keeps running whether or not you stay to watch it: closing the dialog
leaves `Cloning…` in the header, and a page you reload — or a phone that dropped
the tab mid-transfer — picks the same clone back up and still opens the
repository when it lands.

Each project has its own `status`, `log`, and `tree` tabs on the left plus a
terminal panel below. The order is kept on the server, so every device shows the
same arrangement, and it survives a restart (alongside the TUI it lasts the
session). On a narrow window the tab row folds into a dropdown showing the
current project.

## Views

In the `log` tab, selecting a commit opens its changed-file list alongside the
complete commit diff. Select a file to view only that file's change; use `< log`
to return or `all changes` to restore the complete commit diff.

History loads a page at a time, as the TUI's does — scrolling toward the end of
the list fetches the next page, so deep histories stay reachable without loading
them up front. The filter narrows the commits already loaded rather than
searching the server, so paging pauses while a query is up — the list says how
many are loaded, and clearing the filter resumes loading. The list is the history
as of entering the tab: unlike the TUI it does not follow HEAD, so a commit made
in the terminal panel appears after leaving and re-entering the tab.

The diff pane has a toggle (top-right of the pane) that switches between the
inline unified diff and a side-by-side split view, mirroring the TUI's `s`. The
choice lasts the page, the same lifetime the TUI gives it; on a narrow window the
two sides stack — removed above added — rather than sitting side by side, since
neither column would have the width to read.

**Line numbers** ride in a pinned gutter as they do in the TUI: the unified view
shows both sides (old, new), leaving a column blank where the line does not exist
on that side; each split half shows the side it renders; and a file opened from
the tree is numbered by its own lines. The gutter stays put while the code
scrolls sideways, and the numbers stay out of anything you copy.

The `status` list highlights recently touched files the same way the TUI does:
accent-coloured and bold for the first 5 seconds after a file's mtime, accent
until `agent_indicator.hot_window_secs` expires, then plain. The window (and
whether the highlight runs at all) comes from the server's `[agent_indicator]`
settings, so both surfaces fade on the same schedule. Ageing is measured against
the browser's clock, so a device whose time is badly off will fade early or late.

Markdown files (`.md`, `.markdown`) opened from the tree render as formatted
documents by default, with fenced code syntax-highlighted. HTML files (`.html`,
`.htm`) render too, inside a fully sandboxed frame: scripts do not run, and
nothing loads from another host. A page that carries its own styling inline and
embeds images as `data:` URIs shows in full; one that links a stylesheet or images
as separate files shows without them, since repository files are not served to the
frame. This previews a self-contained page rather than a site. A toggle
(top-right of the pane) switches either back to the raw highlighted source.

## Layout

The swatch in the header cycles the accent colour through the same five presets
as the TUI's `<prefix> p` (yellow → cyan → green → magenta → blue) — and it is
the same colour, not a parallel one. The choice is stored on the server
(`~/.nightcrow/viewer.json`), so every device that opens the viewer and every
attached TUI shows it, and a change made anywhere reaches the browsers within a
few seconds and attached terminals immediately. `[theme] name` sets the colour a
session starts with, before anyone has picked one.

Drag the divider between the file list and the diff pane to resize the sidebar,
or double-click it to reset the default width. The width is stored on the server
the same way as the accent, so every device opens at the same split; it is
bounded so the diff pane always keeps at least half the window.

The border between the diff panel and the terminal panel is a divider too: drag
it to give the terminal more or less of the window, double-click to go back to
the default 55/45. It is stored on the server like the sidebar width, so every
browser opens at the same split, and bounded so neither panel shrinks to a sliver
— for "all the way" use the maximize buttons on either panel. Unlike the accent,
this one is **not** shared with an attached TUI: the TUI keeps its own
`[layout] upper_pct`, because the same percentage means a different number of
rows on a terminal than in a browser window, and the terminals' actual size is
already decided by whichever client owns the sizing.

## Terminals

Each terminal pane's toolbar has a **fit to this screen** button, the browser's
half of the TUI's `<prefix> z`. It is offered only while another screen holds the
sizing, because a PTY has one size for the whole session: the panes are fitted to
whichever viewer opened most recently, and everyone else renders that grid until
someone asks for it. Switching projects does not move it, and neither does a
dropped connection coming back — a tab is one screen however many sockets it
opens. Reloading the page counts as opening it, so it takes the sizing again, as
a new tab would.

Drag a terminal pane by its header onto another to reorder the split-view grid;
it works with touch as well as a mouse. The order is kept on the server, so a
refresh, a reconnect, or another device opening the same repository all show the
same arrangement. (It is not written to disk — a server restart clears the
terminals themselves, so there is nothing to persist.)

The **zoom** button on a pane's toolbar fills the panel with that one terminal,
and the keyboard follows it. Like the order, and for the same reason, it is kept
on the server: a refresh comes back to the pane you had zoomed, and another
device showing the same project follows. Opening a terminal ends the zoom — the
new one would be behind it otherwise — and so does closing the zoomed pane. It
is not written to disk either, and cannot be: a zoom names a pane, and
restarting the session ends the panes. An attached TUI keeps its own
`<prefix> f` zoom rather than following this one — the panes are shared, but
what fills a screen is that screen's.

## On a phone

The three regions the desktop shows at once — the file/commit list, the content
pane, and the terminal — would each shrink to an unusable sliver stacked in one
column, so instead a bottom bar switches between them: tap **Files**, **Diff**,
or **Terminal** to give one of them the whole screen. Opening a file or commit
jumps to the content view automatically.

Because a soft keyboard can't type Escape, Tab, Shift-Tab, Ctrl combinations, or
the arrows, the terminal grows a key bar along its bottom on touch devices that
sends those straight to the shell — so you can interrupt a process (`^C`), leave
`vim` (`Esc`), cycle a completion menu backwards (`⇧Tab`), or walk your history
(arrows) without a physical keyboard.

The viewer ships a web-app manifest and icons, so you can **add it to your home
screen** and launch it as a standalone, chrome-less window — more room for the
terminal and one-tap access. On iOS this works over plain HTTP (Safari → *Share*
→ *Add to Home Screen*). Android's install prompt additionally wants a service
worker and a secure origin, so reach the viewer over HTTPS (a reverse proxy or
tunnel) to get it there; the viewer has no offline mode either way — every screen
needs the server.

## Configuration and access

Configure where it listens under `[web_viewer]`:

```toml
[web_viewer]
bind = "127.0.0.1"   # loopback only; change deliberately
port = 8091
# password = "..."   # auto-generated and written here on first launch if unset
```

`--port` and `--bind` override those for one run:

```bash
nightcrow --port 9000
```

Repositories opened or closed in the browser reach every attached terminal, and
are written back to `~/.nightcrow/workspace.json` so the next session starts on
the same set.

**Authentication.** If no `password` is set when the viewer is enabled, a random
one is generated and written back into your config (so it survives restarts and
stays readable) and printed once at startup. To avoid a plaintext password on
disk, set `hashed_password` to an Argon2 PHC string instead — it takes
precedence. Login is rate-limited and grants a session cookie.

> **Security.** The viewer serves repository contents *and* interactive
> terminals, so an authenticated session is equivalent to shell access. It binds
> to loopback (`127.0.0.1`) by default and speaks plain HTTP with **no built-in
> TLS**. For remote access, do **not** expose the port directly — tunnel it over
> SSH (`ssh -L 8091:127.0.0.1:8091 host`) or put it behind a TLS reverse proxy.

## Developing the frontend

The UI lives in `viewer-ui/` (React + Vite + Tailwind). Its build output is
committed to `viewer-ui/dist/` and embedded into the binary, so installing
nightcrow never requires Node.

```bash
npm --prefix viewer-ui install
npm --prefix viewer-ui run dev     # Vite on :5173, proxying the API to :8091
npm --prefix viewer-ui run build   # rebuild dist/ — commit the result
```

CI rebuilds the bundle and fails if it differs from what is committed.

Design notes: [Architecture → Web layer](architecture/web.md).
