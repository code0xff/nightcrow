# Web viewer

A browser surface that renders the same git data as a native web page — selectable text, real scrolling, clickable paths, and a layout that adapts to a phone. It also serves the session's terminals, the same panes an attached TUI sees.

It is always on — it is one of the session's two faces, not an add-on.

## Projects in the browser

The served repositories appear as project tabs in the header — `+ open` browses the server machine's folders to add one, `×` closes it, and dragging a tab reorders them.

The same dialog **clones a git URL** into the folder it is showing: paste `https://…` or `git@host:path`, and the repository opens as a tab when the clone finishes. Cloning runs `git` on the server, so it uses that machine's credentials — an SSH agent, a credential helper — and a private remote works exactly as it would in a shell there. Local paths and git's `ext::` transport are refused. A clone keeps running whether or not you stay to watch it: closing the dialog leaves `Cloning…` in the header, and a page you reload — or a phone that dropped the tab mid-transfer — picks the same clone back up and still opens the repository when it lands.

Each project has its own `status`, `log`, and `tree` tabs on the left plus a terminal panel below. The order is kept on the server, so every device shows the same arrangement, and it survives a restart (alongside the TUI it lasts the session). On a narrow window the tab row folds into a dropdown showing the current project.

## Views

**A project opens onto what it was last showing.** The tab you were in, the file you had open, and the directories the tree had expanded come back when you open that project again — on the next visit, after a reload, and on whatever device you pick up next, since the server keeps it. The TUI has done this since it had a session file; this is the same idea in the browser, kept in the viewer's own file rather than the TUI's, so the two do not overwrite each other. A file that has gone since you left simply does not open: the project comes back to its list, not to an error, and keeps asking for it next time — the server answers a deleted file and one it could not read the same way, so forgetting on the first sign of trouble would throw away a perfectly good memory. On a phone, restoring does not move you: whichever of the three views you were on is the one you stay on, with the file waiting behind it.

In the `log` tab, selecting a commit opens its changed-file list alongside the complete commit diff. Select a file to view only that file's change; use `< log` to return or `all changes` to restore the complete commit diff.

History loads a page at a time, as the TUI's does — scrolling toward the end of the list fetches the next page, so deep histories stay reachable without loading them up front. The filter narrows the commits already loaded rather than searching the server, so paging pauses while a query is up — the list says how many are loaded, and clearing the filter resumes loading. The list follows HEAD the way the TUI's does: a commit made in the terminal panel below appears at the top on its own, without disturbing the pages you have scrolled through. A rewrite of the history you were reading — a rebase, an amend — replaces the list with the new history instead, and closes a commit drill-down whose commit it swept away.

With a diff showing, the content pane has a toggle (top-right) that switches between the inline unified diff and a side-by-side split view, mirroring the TUI's `s`. The choice lasts the page, the same lifetime the TUI gives it; on a narrow window the two sides stack — removed above added — rather than sitting side by side, since neither column would have the width to read.

Beside it, a **whole file** toggle swaps the diff for the file it belongs to, opened at the change that was on screen — the browser's half of the TUI's `v`. It shows the file as the commit left it when you reached the diff from the log, and the working copy when you reached it from the status list, so what you read is what the diff was describing. Press it again for the diff. It appears only where there is a second face to show: a whole-commit diff spans several files, so "which one" has no answer, and a file opened from the tree has no diff behind it. The TUI draws the same two lines.

**Line numbers** ride in a pinned gutter as they do in the TUI: the unified view shows both sides (old, new), leaving a column blank where the line does not exist on that side; each split half shows the side it renders; and a file opened from the tree is numbered by its own lines. The gutter stays put while the code scrolls sideways, and the numbers stay out of anything you copy.

The `status` list highlights recently touched files the same way the TUI does: accent-coloured and bold for the first 5 seconds after a file's mtime, accent until `agent_indicator.hot_window_secs` expires, then plain. The window (and whether the highlight runs at all) comes from the server's `[agent_indicator]` settings, so both surfaces fade on the same schedule. Ageing is measured against the browser's clock, so a device whose time is badly off will fade early or late.

Markdown files (`.md`, `.markdown`) opened from the tree render as formatted documents by default, with fenced code syntax-highlighted. HTML files (`.html`, `.htm`) render too, inside a sandboxed frame that allows the document's own inline scripts and nothing else — so an interactive single-file page works (a slide deck's keyboard navigation, a chart that draws itself), while the frame stays cut off from the session: it runs as no origin, its requests carry no login, and nothing loads from or connects to another host. A page that carries its scripts and styling inline and embeds images as `data:` URIs runs in full; one that links a stylesheet, images, or scripts as separate files (or from a CDN) shows without them. This previews a self-contained page rather than a site. A toggle (top-right of the pane) switches either back to the raw highlighted source. Click the frame first if keys seem to go nowhere — the keyboard follows focus.

## Layout

The swatch in the header cycles the accent colour through the same five presets as the TUI's `<prefix> p` (yellow → cyan → green → magenta → blue) — and it is the same colour, not a parallel one. The choice is stored on the server (`~/.nightcrow/viewer.json`), so every device that opens the viewer and every attached TUI shows it, and a change made anywhere reaches the browsers within a few seconds and attached terminals immediately. `[theme] name` sets the colour a session starts with, before anyone has picked one.

Drag the divider between the sidebar and the content pane to resize the sidebar, or double-click it to reset the default width. The width is stored on the server the same way as the accent, so every device opens at the same split; it is bounded so the content pane always keeps at least half the window.

The border between the upper panel and the terminal panel is a divider too: drag it to give the terminal more or less of the window, double-click to go back to the default 55/45. It is stored on the server like the sidebar width, so every browser opens at the same split, and bounded so neither panel shrinks to a sliver — for "all the way" use the maximize buttons on either panel. Unlike the accent, this one is **not** shared with an attached TUI: the TUI keeps its own `[layout] upper_pct`, because the same percentage means a different number of rows on a terminal than in a browser window, and the terminals' actual size is already decided by whichever client owns the sizing.

## Terminals

Each terminal pane's toolbar has a **fit to this screen** button, the browser's half of the TUI's `<prefix> z`. It is offered only while another screen holds the sizing, because a PTY has one size for the whole session: the panes are fitted to whichever viewer opened most recently, and everyone else renders that grid until someone asks for it. Switching projects does not move it, and neither does a dropped connection coming back — a tab is one screen however many sockets it opens. Reloading the page counts as opening it, so it takes the sizing again, as a new tab would.

Nobody holding the sizing is a state for a session with nobody in it. If every screen goes and one comes back — a phone that slept long enough for its socket to die — it takes the sizing rather than returning as a spectator, because there is no other screen to take it from.

The panel draws its panes either side by side, as the TUI does, or one at a time behind a tab strip. The button beside **+** switches between the two, and a narrow screen starts on tabs — a split grid gives each pane fewer columns than a command line needs. Once you pick, that choice sticks on that device, rotation included; it is stored in the browser rather than on the server, because what a phone should do with four panes is not what the desktop beside it should do.

Tabs change nothing about the session: **+** still opens a terminal that every client sees, the tabs sit in pane order, and a tab you are not looking at is a running program whose output keeps arriving. Every pane is also held at the panel's full size while tabbed, so switching tabs costs no resize — which is the same reason a tabbed browser and an attached TUI cannot both be right about how wide a pane is. Give the sizing to whichever screen you are working on with the button above, or leave the TUI holding it and read the panes at its width.

A tabbed panel shows no **zoom** button — it already shows one pane — and a zoom another client set does not move the keyboard here.

Drag a terminal pane by its header, or by its tab, onto another to reorder them; it works with touch as well as a mouse. The order is kept on the server, so a refresh, a reconnect, or another device opening the same repository all show the same arrangement. (It is not written to disk — a server restart clears the terminals themselves, so there is nothing to persist.)

The **zoom** button on a pane's toolbar fills the panel with that one terminal, and the keyboard follows it. Like the order, and for the same reason, it is kept on the server: a refresh comes back to the pane you had zoomed, and another device showing the same project follows. Opening a terminal ends the zoom — the new one would be behind it otherwise — and so does closing the zoomed pane. It is not written to disk either, and cannot be: a zoom names a pane, and restarting the session ends the panes. An attached TUI keeps its own `<prefix> f` zoom rather than following this one — the panes are shared, but what fills a screen is that screen's.

To copy from a pane, select with the mouse and press the copy key your browser already uses. A plain drag selects only while the pane's program is not reading the mouse itself; most full-screen programs do read it, and then a drag is theirs — that is how clicking a menu in one of them works at all. Hold a modifier to take the drag back for a selection: **Option** on a Mac, **Shift** everywhere else. There is nothing to copy until something is selected, so without the modifier the copy key looks broken rather than empty.

A program running in a pane can also copy on its own — Claude Code's `/copy`, vim's OSC 52 clipboard, tmux's `copy-pipe`. That copy reaches *this* page, not the machine hosting the session, which is what makes it worth having: the `pbcopy` such a program also runs writes to a clipboard nobody at this end can reach. Most of the time it simply happens, including over plain `http://`.

When the browser refuses to fill the clipboard without being asked — Safari wants a press for it, and any browser may — a notice appears with a **Copy** button instead, and pressing it is the press it wanted. It stays up until the text is across, so it is still there if you come back to it.

A program asking to *read* the clipboard is never answered. It would hand whatever was last copied — a password, a token — to whatever is running in the pane, and unlike writing that is something a program could not otherwise get.

## On a phone

The three regions the desktop shows at once — the sidebar, the content pane, and the terminal — would each shrink to an unusable sliver stacked in one column, so instead a bottom bar switches between them: tap **Repo**, **Content**, or **Terminal** to give one of them the whole screen. The labels name the regions rather than what is in them: the sidebar is `status`, `log`, or `tree`, and the content pane holds a diff, a whole file, or nothing yet. Opening a file or commit jumps to the content pane automatically.

**Drag a pane to scroll it.** A finger dragged up or down the terminal turns the same wheel a mouse would, so where it goes is up to the program in the pane: an agent or a pager that reads the wheel itself scrolls its own view, `less` and `man` get the arrow keys they expect under alternate scroll, and a plain shell scrolls the emulator's scrollback. That routing is the browser terminal's, matching what the TUI does with `Shift+↑/↓` — which is why a full-screen program that keeps its transcript in its own memory scrolls at all, rather than dragging an empty scrollback around. A short drag is still a tap, so tapping to place the cursor and pinching to zoom both survive.

Because a soft keyboard can't type Escape, Tab, Shift-Tab, Ctrl combinations, or the arrows, the terminal grows a key bar along its bottom on touch devices that sends those straight to the shell — so you can interrupt a process (`^C`), leave `vim` (`Esc`), reach a tmux session's prefix (`^B`), cycle a completion menu backwards (`⇧Tab`), or walk your history (arrows) without a physical keyboard.

**`Ctrl` on the bar is a latch, not a key.** More combinations matter than there are buttons for, so tapping `Ctrl` lights it up — and puts the keyboard back in the pane, since what spends it is the next character you *type* — after which that character leaves as the combination: `Ctrl` then `a` is `^A`, and so on for anything a terminal has a control byte for, `Ctrl+Space` and `Ctrl+[` included. Type something with no such byte — Hangul, an emoji, more than one character — and it goes through as you typed it. Some input leaves the latch alone altogether — an Escape or an arrow from a hardware keyboard — because what the program in the pane reports back to the browser arrives looking the same, and a latch spent on that would die before you typed anything. So the light is what to read: `Ctrl` is armed for exactly as long as its button is lit, and tapping it again, tapping any other key on the bar, or hiding the bar puts it out.

**The bar is not a phone-width thing** — a tablet is as wide as a laptop and types the same way, so what turns it on is the pointer: any device whose primary pointer is a finger gets it, at any width, along with every window narrower than 768px. The keyboard button in the terminal panel's toolbar turns it off and on from there, and this browser remembers which — so a desktop that wants the keys anyway can keep them, and a tablet with a hardware keyboard attached can drop them.

The viewer ships a web-app manifest and icons, so you can **add it to your home screen** and launch it as a standalone, chrome-less window — more room for the terminal and one-tap access. On iOS this works over plain HTTP (Safari → *Share* → *Add to Home Screen*). Android's install prompt additionally wants a service worker and a secure origin, so reach the viewer over HTTPS (a reverse proxy or tunnel) to get it there; the viewer has no offline mode either way — every screen needs the server.

## Configuration and access

Configure where it listens under `[web_viewer]`:

```toml
[web_viewer]
bind = "127.0.0.1"   # loopback only; change deliberately
port = 8091
# password = "..."   # auto-generated and written here on first launch if unset
session_ttl_hours = 24   # how long a login lasts; 0 = never expires
```

`--port` and `--bind` override those for one run:

```bash
nightcrow --port 9000
```

Repositories opened or closed in the browser reach every attached terminal, and are written back to `~/.nightcrow/workspace.json` so the next session starts on the same set.

**Authentication.** If no `password` is set when the viewer is enabled, a random one is generated and written back into your config (so it survives restarts and stays readable) and printed once at startup. To avoid a plaintext password on disk, set `hashed_password` to an Argon2 PHC string instead — it takes precedence. Login is rate-limited and grants a session cookie. Sessions survive a daemon restart: tokens are persisted to `~/.nightcrow/sessions` with owner-only file permissions. Logout revokes the token server-side, so clearing the cookie alone is not enough to invalidate a session.

**How long a login lasts** is `session_ttl_hours`, 24 hours by default. `session_ttl_hours = 0` means it never expires on its own — logging out, or deleting `~/.nightcrow/sessions`, is then the only thing that ends a session. Whether repeating the login buys anything is a judgement about your own setup: on a loopback-bound session there may be nobody to re-authenticate against, while a viewer reachable from another machine is shell access that a stolen cookie opens. Two things to know either way:

- **Lowering it reaches logins already handed out**, from the next restart — each one's deadline is brought down to the new lifetime. Raising it never pushes an existing deadline further away; only a fresh login gets the longer one.
- **The cookie asks for at most 400 days** whatever the setting says. That is the ceiling RFC 6265bis puts on `Max-Age`, and Chrome has enforced it since version 104, so asking for more would be silently reduced there and honoured elsewhere. A session with no expiry stays valid on the server past that — it is the browser that will have forgotten the cookie, so you log in again.

`[web_viewer]` is not re-read by a config reload — the listener is already bound — so a change here takes effect when the session restarts.

> **Security.** The viewer serves repository contents *and* interactive terminals, so an authenticated session is equivalent to shell access. It binds to loopback (`127.0.0.1`) by default and speaks plain HTTP with **no built-in TLS**. For remote access, do **not** expose the port directly — tunnel it over SSH (`ssh -L 8091:127.0.0.1:8091 host`) or put it behind a TLS reverse proxy.

## Developing the frontend

The UI lives in `viewer-ui/` (React + Vite + Tailwind). Its build output is committed to `viewer-ui/dist/` and embedded into the binary, so installing nightcrow never requires Node.

```bash
npm --prefix viewer-ui install
npm --prefix viewer-ui run dev     # Vite on :5173, proxying the API to :8091
npm --prefix viewer-ui run build   # rebuild dist/ — commit the result
```

CI rebuilds the bundle and fails if it differs from what is committed.

**A tab open across a rebuild is told so.** Every reply to the poll the page already makes names the build it was served with, so within a few seconds of a rebuild the tab raises a notice with a **Reload** button and keeps it up until you act on it. Nothing reloads itself: a tab that did would take away whatever was being typed into a terminal, and being one build behind is not urgent enough to interrupt anyone.

**Until you do, the tab is still running the bundle it loaded.** Chunk names carry a content hash, so a build replaces them rather than overwriting them, and the markdown renderer, the HTML preview, and the terminal panel are each fetched only when first needed — so one you open after the rebuild is simply gone. That pane then says part of the app could not be loaded and offers the same reload. (The same message covers a server that has become unreachable, since the browser reports both the same way — if the reload fails too, that is which one it was.)

**What counts as a rebuild depends on the server.** A debug server reads `dist` from disk, so `npm --prefix viewer-ui run build` is the whole of it — reload the tab and you are current. A release binary carries the bundle inside it and a running process keeps the one it started with, so [an update](getting-started.md#updating) changes nothing until the session is restarted; that is the heavier move, since stopping the session ends its terminals, and it is why the notice can only appear afterwards. Reloading the tab never costs you anything — the same repositories, the same terminals, and the pane you were typing in.

Design notes: [Architecture → Web layer](architecture/web.md).
