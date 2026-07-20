import { useEffect, useRef, useState } from "react";
import { Terminal } from "@xterm/xterm";
import { FitAddon } from "@xterm/addon-fit";

/**
 * One WebSocket multiplexes every terminal for a repository.
 *
 * Output arrives as binary frames tagged with a 4-byte little-endian pane id
 * (see src/web/viewer/terminal.rs) — binary rather than JSON because PTY reads
 * routinely split a multi-byte sequence, and decoding early would corrupt it
 * before xterm.js could reassemble it. Bytes are handed to xterm.js untouched.
 */
export function TerminalPanel({ repo }: { repo: string }) {
  const hostRef = useRef<HTMLDivElement>(null);
  const socketRef = useRef<WebSocket | null>(null);
  const termsRef = useRef(new Map<number, { term: Terminal; fit: FitAddon }>());
  const [panes, setPanes] = useState<number[]>([]);
  const [active, setActive] = useState<number | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    const scheme = location.protocol === "https:" ? "wss:" : "ws:";
    const socket = new WebSocket(
      `${scheme}//${location.host}/ws/term?repo=${encodeURIComponent(repo)}`,
    );
    socket.binaryType = "arraybuffer";
    socketRef.current = socket;

    socket.onmessage = (event) => {
      if (typeof event.data === "string") {
        const message = JSON.parse(event.data);
        if (message.type === "created") {
          setPanes((current) => [...current, message.pane]);
          setActive((current) => current ?? message.pane);
        } else if (message.type === "exited") {
          termsRef.current.get(message.pane)?.term.dispose();
          termsRef.current.delete(message.pane);
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
      termsRef.current.get(pane)?.term.write(frame.subarray(4));
    };
    socket.onclose = () => setError("terminal connection closed");

    return () => {
      socket.close();
      termsRef.current.forEach(({ term }) => term.dispose());
      termsRef.current.clear();
    };
  }, [repo]);

  // Attach a real xterm instance once its host element exists.
  useEffect(() => {
    if (active === null || !hostRef.current) return;
    let entry = termsRef.current.get(active);
    if (!entry) {
      const term = new Terminal({
        fontFamily: getComputedStyle(document.body).fontFamily,
        fontSize: 13,
        theme: { background: "#0b0b0d", foreground: "#e6e6ec" },
        cursorBlink: true,
      });
      const fit = new FitAddon();
      term.loadAddon(fit);
      term.onData((data) =>
        socketRef.current?.send(
          JSON.stringify({ type: "input", pane: active, data }),
        ),
      );
      entry = { term, fit };
      termsRef.current.set(active, entry);
    }
    hostRef.current.replaceChildren();
    entry.term.open(hostRef.current);
    entry.fit.fit();
    entry.term.focus();
    socketRef.current?.send(
      JSON.stringify({
        type: "resize",
        pane: active,
        rows: entry.term.rows,
        cols: entry.term.cols,
      }),
    );
  }, [active]);

  // Keep the PTY's idea of the window in step with the browser's.
  useEffect(() => {
    const onResize = () => {
      if (active === null) return;
      const entry = termsRef.current.get(active);
      if (!entry) return;
      entry.fit.fit();
      socketRef.current?.send(
        JSON.stringify({
          type: "resize",
          pane: active,
          rows: entry.term.rows,
          cols: entry.term.cols,
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

  return (
    <section className="flex min-h-0 flex-col border-t border-ink-700">
      <div className="flex shrink-0 items-center gap-1 bg-ink-900 px-2 py-1">
        {panes.map((pane, index) => (
          <button
            key={pane}
            onClick={() => setActive(pane)}
            className={`rounded-sm px-2 py-0.5 text-xs ${
              pane === active
                ? "bg-ink-700 text-ink-50"
                : "text-ink-400 hover:text-ink-200"
            }`}
          >
            term {index + 1}
          </button>
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
      <div className="min-h-0 flex-1 overflow-hidden bg-ink-950 p-1">
        {active === null ? (
          <p className="p-3 text-ink-400">
            No terminal open. Press <span className="text-accent">+</span> to
            start one.
          </p>
        ) : (
          <div ref={hostRef} className="h-full" />
        )}
      </div>
    </section>
  );
}
