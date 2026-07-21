import { useEffect, useRef, useState } from "react";
import { Terminal } from "@xterm/xterm";
import { FitAddon } from "@xterm/addon-fit";
import { MaximizeIcon } from "./icons";

interface PaneView {
  term: Terminal;
  fit: FitAddon;
  el: HTMLDivElement;
}

/// Tab labels are capped by display width (not character count) so a title of
/// wide CJK glyphs cannot overflow the tab bar; the full title stays reachable
/// through the tab's tooltip. Matches the viewer's tab-label convention.
const TAB_TITLE_MAX_CELLS = 20;

/// True for code points that occupy two terminal cells. An approximation of the
/// common East Asian wide / fullwidth ranges — enough to keep CJK titles from
/// overflowing without pulling in a full Unicode width table.
function isWide(cp: number): boolean {
  return (
    (cp >= 0x1100 && cp <= 0x115f) ||
    (cp >= 0x2e80 && cp <= 0x303e) ||
    (cp >= 0x3041 && cp <= 0x33ff) ||
    (cp >= 0x3400 && cp <= 0x4dbf) ||
    (cp >= 0x4e00 && cp <= 0x9fff) ||
    (cp >= 0xa000 && cp <= 0xa4cf) ||
    (cp >= 0xac00 && cp <= 0xd7a3) ||
    (cp >= 0xf900 && cp <= 0xfaff) ||
    (cp >= 0xfe30 && cp <= 0xfe4f) ||
    (cp >= 0xff00 && cp <= 0xff60) ||
    (cp >= 0xffe0 && cp <= 0xffe6) ||
    (cp >= 0x1f300 && cp <= 0x1faff) ||
    (cp >= 0x20000 && cp <= 0x3fffd)
  );
}

/// Truncate `text` to at most `max` display cells, appending an ellipsis (which
/// costs one cell) when anything was dropped.
function truncateCells(text: string, max: number): string {
  let width = 0;
  for (const ch of text) width += isWide(ch.codePointAt(0) ?? 0) ? 2 : 1;
  if (width <= max) return text;

  let used = 0;
  let out = "";
  for (const ch of text) {
    const cw = isWide(ch.codePointAt(0) ?? 0) ? 2 : 1;
    if (used + cw > max - 1) break;
    out += ch;
    used += cw;
  }
  return `${out}…`;
}

/**
 * One WebSocket multiplexes every terminal for a repository.
 *
 * Output arrives as binary frames tagged with a 4-byte little-endian pane id
 * (see src/web/viewer/terminal.rs) — binary rather than JSON because PTY reads
 * routinely split a multi-byte sequence, and decoding early would corrupt it
 * before xterm.js could reassemble it. Bytes are handed to xterm.js untouched.
 *
 * Each pane owns a dedicated child element that lives for the pane's lifetime.
 * xterm's `open()` is called exactly once per instance: re-opening a terminal
 * whose element was detached renders blank, so switching panes toggles
 * `display` instead of moving a single terminal between elements.
 */
export function TerminalPanel({
  repo,
  maximized,
  onToggleMaximized,
}: {
  repo: string;
  maximized: boolean;
  onToggleMaximized: () => void;
}) {
  const hostRef = useRef<HTMLDivElement>(null);
  const socketRef = useRef<WebSocket | null>(null);
  const viewsRef = useRef(new Map<number, PaneView>());
  // Output for a pane whose xterm view does not exist yet. A pane's view is
  // materialised in a later effect (after its "created" updates React state),
  // but the replayed scrollback arrives on the socket immediately after that
  // message — buffer it here and flush when the view is opened, or it is lost.
  const pendingRef = useRef(new Map<number, Uint8Array[]>());
  const [panes, setPanes] = useState<number[]>([]);
  const [active, setActive] = useState<number | null>(null);
  // Per-pane title from the shell's OSC 0/2 sequence (parsed by xterm.js), so a
  // tab reads e.g. "claude" or "vim README" instead of a bare "term 2".
  const [titles, setTitles] = useState<Record<number, string>>({});
  const [error, setError] = useState<string | null>(null);

  // One socket per repo. Pane ids belong to a repository's own terminal hub, so
  // switching repos must reset the pane list and dispose the old terminals —
  // otherwise stale ids point at panes the new repo never created.
  useEffect(() => {
    let closedByUs = false;
    let reconnectTimer: ReturnType<typeof setTimeout> | undefined;
    setError(null);

    const disposeAll = () => {
      viewsRef.current.forEach((view) => view.term.dispose());
      viewsRef.current.clear();
      pendingRef.current.clear();
      hostRef.current?.replaceChildren();
    };

    const connect = () => {
      // Each (re)connection starts from a clean slate and lets the server
      // repopulate it: on connect the hub replays every live pane and its
      // scrollback, so a browser refresh restores the terminals while a server
      // restart (no panes to replay) correctly comes back empty. Keeping stale
      // local panes would instead point at terminals the new socket never
      // announced.
      setPanes([]);
      setActive(null);
      setTitles({});
      disposeAll();

      const scheme = location.protocol === "https:" ? "wss:" : "ws:";
      const socket = new WebSocket(
        `${scheme}//${location.host}/ws/term?repo=${encodeURIComponent(repo)}`,
      );
      socket.binaryType = "arraybuffer";
      socketRef.current = socket;

      socket.onopen = () => setError(null);
      socket.onmessage = (event) => {
        if (typeof event.data === "string") {
          const message = JSON.parse(event.data);
          if (message.type === "created") {
            // Focus follows creation: a freshly opened terminal becomes active.
            setPanes((current) => [...current, message.pane]);
            setActive(message.pane);
          } else if (message.type === "exited") {
            setPanes((current) => current.filter((p) => p !== message.pane));
            setActive((current) => (current === message.pane ? null : current));
            pendingRef.current.delete(message.pane);
            setTitles((current) => {
              if (!(message.pane in current)) return current;
              const next = { ...current };
              delete next[message.pane];
              return next;
            });
          } else if (message.type === "error") {
            setError(message.message);
          }
          return;
        }
        const frame = new Uint8Array(event.data as ArrayBuffer);
        if (frame.length < 4) return;
        const pane = new DataView(frame.buffer).getUint32(0, true);
        const bytes = frame.subarray(4);
        const view = viewsRef.current.get(pane);
        if (view) {
          view.term.write(bytes);
        } else {
          // The view is created by a later effect; hold this until then.
          const queue = pendingRef.current.get(pane) ?? [];
          queue.push(bytes);
          pendingRef.current.set(pane, queue);
        }
      };
      // Reconnect quietly. The control socket is always open — it is how a
      // terminal gets created — so a drop with nothing running is not worth
      // alarming the user about; just wait and retry. A restart thus heals
      // into a clean, empty panel rather than a stuck error.
      socket.onclose = () => {
        if (closedByUs) return;
        reconnectTimer = setTimeout(connect, 1000);
      };
    };

    connect();

    return () => {
      closedByUs = true;
      if (reconnectTimer) clearTimeout(reconnectTimer);
      socketRef.current?.close();
      disposeAll();
    };
  }, [repo]);

  // Materialise one xterm per pane in its own child element, and dispose the
  // views of panes that have gone away. `open()` runs once, here, and never
  // again for that instance.
  useEffect(() => {
    const host = hostRef.current;
    if (!host) return;

    for (const pane of panes) {
      if (viewsRef.current.has(pane)) continue;
      const el = document.createElement("div");
      el.style.height = "100%";
      el.style.display = "none";
      host.appendChild(el);

      const term = new Terminal({
        fontFamily: getComputedStyle(document.body).fontFamily,
        fontSize: 12,
        theme: { background: "#0b0b0d", foreground: "#e6e6ec" },
        cursorBlink: true,
      });
      const fit = new FitAddon();
      term.loadAddon(fit);
      term.onData((data) =>
        socketRef.current?.send(
          JSON.stringify({ type: "input", pane, data }),
        ),
      );
      // xterm parses OSC 0/2 window-title sequences; mirror the latest non-empty
      // one into the tab label. An empty title is ignored so the previous label
      // (or the "term N" fallback) stands, matching the TUI.
      term.onTitleChange((title) => {
        const cleaned = title.replace(/\s+/g, " ").trim();
        if (!cleaned) return;
        setTitles((current) => ({ ...current, [pane]: cleaned }));
      });
      term.open(el);
      viewsRef.current.set(pane, { term, fit, el });

      // Flush any output (typically replayed scrollback) that arrived before
      // this view existed, in order, so the restored screen is complete.
      const queued = pendingRef.current.get(pane);
      if (queued) {
        for (const chunk of queued) term.write(chunk);
        pendingRef.current.delete(pane);
      }
    }

    for (const [pane, view] of viewsRef.current) {
      if (!panes.includes(pane)) {
        view.term.dispose();
        view.el.remove();
        viewsRef.current.delete(pane);
      }
    }
  }, [panes]);

  // Show only the active pane; fit it to the panel and tell the PTY the size.
  // A hidden element has no dimensions, so only the visible pane is ever fit.
  useEffect(() => {
    for (const [pane, view] of viewsRef.current) {
      view.el.style.display = pane === active ? "block" : "none";
    }
    if (active === null) return;
    const view = viewsRef.current.get(active);
    if (!view) return;
    view.fit.fit();
    view.term.focus();
    socketRef.current?.send(
      JSON.stringify({
        type: "resize",
        pane: active,
        rows: view.term.rows,
        cols: view.term.cols,
      }),
    );
  }, [active, panes]);

  // When the active terminal is closed (or exits) but others remain, focus
  // falls back to one of them rather than leaving the panel blank.
  useEffect(() => {
    if (active === null && panes.length > 0) {
      setActive(panes[panes.length - 1]);
    }
  }, [active, panes]);

  // Keep the active PTY's idea of its window in step with the panel's actual
  // size. Observing the host rather than listening for window resizes catches
  // every way the panel can change shape — a layout change, a split, a font
  // change — not just the one that happens to move the viewport.
  useEffect(() => {
    const host = hostRef.current;
    if (!host || active === null) return;

    // The grid is what the PTY cares about; pixel changes that leave rows and
    // cols alone would just be noise on the socket and extra SIGWINCHs in the
    // shell. A drag crosses a cell boundary rarely, so this filters most of it.
    let sent = { rows: 0, cols: 0 };
    const observer = new ResizeObserver(() => {
      const view = viewsRef.current.get(active);
      if (!view) return;
      // A collapsed panel — height 0 while the file pane is maximised — would
      // make fit propose a one-row terminal and SIGWINCH the shell to a garbage
      // size, corrupting any full-screen program running in it. There is
      // nothing to fit to at zero size; the restore fires its own resize.
      if (host.clientHeight === 0 || host.clientWidth === 0) return;
      view.fit.fit();
      const { rows, cols } = view.term;
      if (rows === sent.rows && cols === sent.cols) return;
      sent = { rows, cols };
      socketRef.current?.send(
        JSON.stringify({ type: "resize", pane: active, rows, cols }),
      );
    });
    observer.observe(host);
    return () => observer.disconnect();
  }, [active]);

  const create = () => {
    setError(null);
    socketRef.current?.send(
      JSON.stringify({ type: "create", rows: 24, cols: 80 }),
    );
  };

  // Ask the server to kill the PTY. The tab is removed when the resulting
  // "exited" broadcast arrives, so every client stays in step.
  const closePane = (pane: number) => {
    socketRef.current?.send(JSON.stringify({ type: "close", pane }));
  };

  return (
    <section className="flex min-h-0 flex-col border-t border-ink-700">
      <div className="flex shrink-0 items-center gap-1 bg-ink-900 px-2 py-1">
        {/* The tabs scroll; the maximize button is pinned outside them. With up
            to MAX_PTYS_PER_REPO tabs on a narrow panel they would otherwise push
            the button off-screen, leaving no way to restore a maximized panel. */}
        <div className="flex min-w-0 flex-1 items-center gap-1 overflow-x-auto">
          {panes.map((pane, index) => {
            const label = titles[pane] ?? `term ${index + 1}`;
            return (
            <div
              key={pane}
              className={`flex shrink-0 items-center rounded-sm text-xs ${
                pane === active
                  ? "bg-ink-700 text-ink-50"
                  : "text-ink-400 hover:text-ink-200"
              }`}
            >
              <button
                onClick={() => setActive(pane)}
                title={label}
                className="py-0.5 pl-2 pr-1"
              >
                {truncateCells(label, TAB_TITLE_MAX_CELLS)}
              </button>
              <button
                onClick={(e) => {
                  e.stopPropagation();
                  closePane(pane);
                }}
                title="Close terminal"
                aria-label={`close terminal ${index + 1}`}
                className="px-1 py-0.5 text-ink-400 hover:text-removed"
              >
                ×
              </button>
            </div>
            );
          })}
          <button
            onClick={create}
            className="shrink-0 rounded-sm px-2 py-0.5 text-xs text-ink-400 hover:text-accent"
            title="New terminal"
          >
            +
          </button>
          {error && (
            <span className="ml-2 shrink-0 text-xs text-removed">{error}</span>
          )}
        </div>
        {/* No Escape shortcut to leave: Escape belongs to whatever is running
            in the PTY, and stealing it would break vim and every TUI below it.
            The button is the way out. */}
        <button
          onClick={onToggleMaximized}
          aria-pressed={maximized}
          title={maximized ? "Restore panel height" : "Maximize the panel"}
          aria-label={maximized ? "Restore panel height" : "Maximize the panel"}
          className="flex shrink-0 items-center rounded-sm px-1.5 py-0.5 text-ink-400 hover:text-accent"
        >
          <MaximizeIcon maximized={maximized} />
        </button>
      </div>
      <div className="relative min-h-0 flex-1 overflow-hidden bg-ink-950 p-1">
        {panes.length === 0 && (
          <p className="p-3 text-ink-400">
            No terminal open. Press <span className="text-accent">+</span> to
            start one.
          </p>
        )}
        <div ref={hostRef} className="h-full" />
      </div>
    </section>
  );
}
