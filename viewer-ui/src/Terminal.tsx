import { useEffect, useRef, useState, type CSSProperties } from "react";
import { MaximizeIcon, PlusIcon } from "./icons";
import { planLayout, type PaneView } from "./terminalLayout";
import { usePaneDrag } from "./usePaneDrag";
import { useTerminalSocket } from "./useTerminalSocket";
import { useTerminalViews } from "./useTerminalViews";
import { TerminalCell } from "./TerminalCell";

/**
 * One WebSocket multiplexes every terminal for a repository.
 *
 * Output arrives as binary frames tagged with a 4-byte little-endian pane id
 * (see src/web/viewer/terminal.rs) — binary rather than JSON because PTY reads
 * routinely split a multi-byte sequence, and decoding early would corrupt it
 * before xterm.js could reassemble it. Bytes are handed to xterm.js untouched.
 *
 * Panes render simultaneously in a balanced split-view grid (mirroring the
 * TUI), not tabs. Every pane's cell stays mounted for its lifetime — xterm's
 * `open()` runs once per instance and re-opening a detached element renders
 * blank — so reflowing the grid only restyles the (stable, keyed) cells, and
 * zooming a pane toggles the others' `display` rather than unmounting them.
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
  const containerRef = useRef<HTMLDivElement>(null);
  const socketRef = useRef<WebSocket | null>(null);
  const viewsRef = useRef(new Map<number, PaneView>());
  // The DOM element xterm is opened into, per pane, registered by each cell.
  const bodyRefs = useRef(new Map<number, HTMLDivElement>());
  // Last size reported to each PTY, so a reflow that leaves rows/cols unchanged
  // does not spam resize frames.
  const sentSizesRef = useRef(new Map<number, { rows: number; cols: number }>());
  // Output for a pane whose xterm view does not exist yet. A pane's view is
  // materialised in a later effect (after its "created" updates React state),
  // but the replayed scrollback arrives on the socket immediately after that
  // message — buffer it here and flush when the view is opened, or it is lost.
  const pendingRef = useRef(new Map<number, Uint8Array[]>());
  // The pane a client last focused, per repo. This panel instance is reused
  // across project switches (it is not keyed by repo), so this survives the
  // reconnect and lets us restore the selection instead of jumping to the last
  // replayed pane.
  const lastActiveByRepoRef = useRef(new Map<string, number>());
  // Count of creates this client has requested but not yet seen announced.
  // Focus follows only these — not panes replayed on reconnect, startup
  // terminals, or another browser's creates.
  const expectCreateRef = useRef(0);
  const [panes, setPanes] = useState<number[]>([]);
  const [active, setActive] = useState<number | null>(null);
  // When set, this pane fills the whole panel and the rest are hidden — the
  // web equivalent of the TUI's zoom mode.
  const [zoomed, setZoomed] = useState<number | null>(null);
  // Panel dimensions, tracked so the two-pane split can flip between side-by-side
  // and stacked and so a resize refits every visible pane.
  const [size, setSize] = useState({ w: 0, h: 0 });
  // Per-pane title from the shell's OSC 0/2 sequence (parsed by xterm.js), so a
  // cell reads e.g. "claude" or "vim README" instead of a bare "term 2".
  const [titles, setTitles] = useState<Record<number, string>>({});

  useTerminalSocket({
    repo,
    socketRef,
    viewsRef,
    pendingRef,
    sentSizesRef,
    lastActiveByRepoRef,
    expectCreateRef,
    setPanes,
    setActive,
    setZoomed,
    setTitles,
  });

  // Materialise one xterm per pane, opened into that pane's cell body (rendered
  // below, keyed by pane so it survives grid reflows). `open()` runs once here;
  // dispose the views of panes that have gone away.
  useTerminalViews({
    panes,
    socketRef,
    viewsRef,
    bodyRefs,
    pendingRef,
    setTitles,
  });

  // Fit every visible pane to its cell and report the size to its PTY. Runs on
  // any layout change (pane added/removed, zoom toggled, panel resized). Hidden
  // or collapsed cells (zoomed-out, or the panel shrunk to nothing) report zero
  // size and are skipped — fitting them would SIGWINCH the shell to garbage.
  useEffect(() => {
    for (const [pane, view] of viewsRef.current) {
      const body = bodyRefs.current.get(pane);
      if (!body || body.clientHeight === 0 || body.clientWidth === 0) continue;
      view.fit.fit();
      const { rows, cols } = view.term;
      const sent = sentSizesRef.current.get(pane);
      if (sent && sent.rows === rows && sent.cols === cols) continue;
      sentSizesRef.current.set(pane, { rows, cols });
      socketRef.current?.send(
        JSON.stringify({ type: "resize", pane, rows, cols }),
      );
    }
  }, [panes, zoomed, size]);

  // Track the panel's size so the two-pane split can pick its orientation and a
  // resize refits every pane.
  useEffect(() => {
    const container = containerRef.current;
    if (!container) return;
    const observer = new ResizeObserver(() => {
      setSize({ w: container.clientWidth, h: container.clientHeight });
    });
    observer.observe(container);
    return () => observer.disconnect();
  }, []);

  // When nothing is selected but panes exist — after a close, or after a
  // reconnect reset — pick one: the repo's remembered pane if it is still here,
  // otherwise the last.
  useEffect(() => {
    if (active === null && panes.length > 0) {
      const remembered = lastActiveByRepoRef.current.get(repo);
      setActive(
        remembered !== undefined && panes.includes(remembered)
          ? remembered
          : panes[panes.length - 1],
      );
    }
  }, [active, panes, repo]);

  // Give the keyboard to the active pane.
  useEffect(() => {
    if (active !== null) viewsRef.current.get(active)?.term.focus();
  }, [active]);

  // Select a pane and remember it as this repo's focus, so returning to the
  // project restores it.
  const focusPane = (pane: number) => {
    setActive(pane);
    lastActiveByRepoRef.current.set(repo, pane);
  };

  const create = () => {
    const socket = socketRef.current;
    if (!socket) return;
    // Show the new pane in the grid rather than under whatever was zoomed.
    setZoomed(null);
    // Focus should follow this create when its "created" comes back.
    expectCreateRef.current += 1;
    socket.send(JSON.stringify({ type: "create", rows: 24, cols: 80 }));
  };

  // Ask the server to kill the PTY. The pane is removed when the resulting
  // "exited" broadcast arrives, so every client stays in step.
  const closePane = (pane: number) => {
    socketRef.current?.send(JSON.stringify({ type: "close", pane }));
  };

  const {
    draggingPane,
    dragOverPane,
    reorderable,
    endPaneDrag,
    onPaneDragStart,
    onPaneDragMove,
    onPaneDragEnd,
  } = usePaneDrag({
    panes,
    zoomed,
    onFocus: focusPane,
    onReorder: (order) =>
      socketRef.current?.send(JSON.stringify({ type: "reorder", order })),
  });

  const layout = planLayout(panes.length, size.w >= size.h);

  return (
    <section className="flex min-h-0 flex-col border-t border-ink-700">
      <div className="flex shrink-0 items-center gap-2 bg-ink-900 px-2 py-1">
        {/* The panel's controls sit together at the trailing edge, the way an
            editor keeps a pane's actions. No label: beside the maximise button
            it reads as one of a pair of controls rather than a stray word, and
            the panel it adds to is the thing it points at. `aria-label` is what
            names it, an icon having no text of its own. */}
        <button
          onClick={create}
          title="New terminal"
          aria-label="New terminal"
          className="ml-auto flex shrink-0 items-center rounded-sm px-1.5 py-0.5 text-ink-400 hover:text-accent"
        >
          <PlusIcon />
        </button>
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
            No terminal open. Press <span className="text-accent">+</span> above
            to start one.
          </p>
        )}
        <div
          ref={containerRef}
          className="grid h-full gap-1"
          style={
            zoomed !== null
              ? { gridTemplateColumns: "1fr", gridTemplateRows: "1fr" }
              : {
                  gridTemplateColumns: `repeat(${layout.cols}, minmax(0, 1fr))`,
                  gridTemplateRows: `repeat(${layout.rows}, minmax(0, 1fr))`,
                }
          }
        >
          {panes.map((pane, index) => {
            const label = titles[pane] ?? `term ${index + 1}`;
            const cell = layout.cells[index];
            const cellStyle: CSSProperties =
              zoomed !== null
                ? { display: pane === zoomed ? "flex" : "none" }
                : {
                    display: "flex",
                    gridColumn: `${cell.colStart} / span ${cell.colSpan}`,
                    gridRow: `${cell.row}`,
                  };
            return (
              <TerminalCell
                key={pane}
                pane={pane}
                index={index}
                label={label}
                cellStyle={cellStyle}
                isActive={pane === active}
                isZoomed={zoomed === pane}
                isDragged={draggingPane === pane}
                isDropTarget={dragOverPane === pane}
                reorderable={reorderable}
                onFocus={() => focusPane(pane)}
                onToggleZoom={() =>
                  setZoomed((z) => (z === pane ? null : pane))
                }
                onClose={() => closePane(pane)}
                onPaneDragStart={(e) => onPaneDragStart(e, pane)}
                onPaneDragMove={onPaneDragMove}
                onPaneDragEnd={onPaneDragEnd}
                onPaneDragCancel={endPaneDrag}
                bodyRef={(node) => {
                  if (node) bodyRefs.current.set(pane, node);
                  else bodyRefs.current.delete(pane);
                }}
              />
            );
          })}
        </div>
      </div>
    </section>
  );
}