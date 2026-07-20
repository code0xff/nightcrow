import { useEffect, useRef, useState } from "react";
import { Terminal } from "@xterm/xterm";
import { FitAddon } from "@xterm/addon-fit";

interface PaneView {
  term: Terminal;
  fit: FitAddon;
  el: HTMLDivElement;
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
export function TerminalPanel({ repo }: { repo: string }) {
  const hostRef = useRef<HTMLDivElement>(null);
  const socketRef = useRef<WebSocket | null>(null);
  const viewsRef = useRef(new Map<number, PaneView>());
  const [panes, setPanes] = useState<number[]>([]);
  const [active, setActive] = useState<number | null>(null);
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
      hostRef.current?.replaceChildren();
    };

    const connect = () => {
      // Each (re)connection starts clean: a server restart drops every PTY, so
      // stale panes would point at terminals the new process never created.
      setPanes([]);
      setActive(null);
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
          } else if (message.type === "error") {
            setError(message.message);
          }
          return;
        }
        const frame = new Uint8Array(event.data as ArrayBuffer);
        if (frame.length < 4) return;
        const pane = new DataView(frame.buffer).getUint32(0, true);
        viewsRef.current.get(pane)?.term.write(frame.subarray(4));
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
      term.open(el);
      viewsRef.current.set(pane, { term, fit, el });
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

  // Keep the active PTY's idea of the window in step with the browser's.
  useEffect(() => {
    const onResize = () => {
      if (active === null) return;
      const view = viewsRef.current.get(active);
      if (!view) return;
      view.fit.fit();
      socketRef.current?.send(
        JSON.stringify({
          type: "resize",
          pane: active,
          rows: view.term.rows,
          cols: view.term.cols,
        }),
      );
    };
    window.addEventListener("resize", onResize);
    return () => window.removeEventListener("resize", onResize);
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
        {panes.map((pane, index) => (
          <div
            key={pane}
            className={`flex items-center rounded-sm text-xs ${
              pane === active
                ? "bg-ink-700 text-ink-50"
                : "text-ink-400 hover:text-ink-200"
            }`}
          >
            <button onClick={() => setActive(pane)} className="py-0.5 pl-2 pr-1">
              term {index + 1}
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
        ))}
        <button
          onClick={create}
          className="rounded-sm px-2 py-0.5 text-xs text-ink-400 hover:text-accent"
          title="New terminal"
        >
          +
        </button>
        {error && <span className="ml-2 text-xs text-removed">{error}</span>}
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
