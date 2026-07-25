import { useEffect, useRef, useState, type CSSProperties } from "react";
import { MaximizeIcon, PlusIcon } from "../icons";
import { planLayout, type PaneView } from "../../lib/terminalLayout";
import { usePaneDrag } from "../../hooks/terminal/usePaneDrag";
import { useTerminalSocket } from "../../hooks/terminal/useTerminalSocket";
import { useTerminalViews } from "../../hooks/terminal/useTerminalViews";
import { TerminalCell } from "./TerminalCell";
import { TERM_KEY_BAR, termKeySequence } from "../../lib/termKeys";

export function TerminalPanel({
  repo,
  maximized,
  onToggleMaximized,
  className = "",
}: {
  repo: string;
  maximized: boolean;
  onToggleMaximized: () => void;
  className?: string;
}) {
  const containerRef = useRef<HTMLDivElement>(null);
  const socketRef = useRef<WebSocket | null>(null);
  const viewsRef = useRef(new Map<number, PaneView>());
  const bodyRefs = useRef(new Map<number, HTMLDivElement>());
  // Avoid redundant PTY resize frames.
  const sentSizesRef = useRef(new Map<number, { rows: number; cols: number }>());
  // Buffer scrollback received before the corresponding xterm exists.
  const pendingRef = useRef(new Map<number, Uint8Array[]>());
  // Restore focus when returning to a repository.
  const lastActiveByRepoRef = useRef(new Map<string, number>());
  // Focus only panes created by this client, not replayed panes.
  const expectCreateRef = useRef(0);
  const [panes, setPanes] = useState<number[]>([]);
  const [active, setActive] = useState<number | null>(null);
  const [zoomed, setZoomed] = useState<number | null>(null);
  const [size, setSize] = useState({ w: 0, h: 0 });
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

  useTerminalViews({
    panes,
    socketRef,
    viewsRef,
    bodyRefs,
    pendingRef,
    setTitles,
  });

  // Fit visible panes after layout changes; never resize hidden cells to zero.
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

  useEffect(() => {
    const container = containerRef.current;
    if (!container) return;
    const observer = new ResizeObserver(() => {
      setSize({ w: container.clientWidth, h: container.clientHeight });
    });
    observer.observe(container);
    return () => observer.disconnect();
  }, []);

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

  useEffect(() => {
    if (active !== null) viewsRef.current.get(active)?.term.focus();
  }, [active]);

  const focusPane = (pane: number) => {
    setActive(pane);
    lastActiveByRepoRef.current.set(repo, pane);
  };

  const create = () => {
    const socket = socketRef.current;
    if (!socket) return;
    setZoomed(null);
    expectCreateRef.current += 1;
    socket.send(JSON.stringify({ type: "create", rows: 24, cols: 80 }));
  };

  const closePane = (pane: number) => {
    socketRef.current?.send(JSON.stringify({ type: "close", pane }));
  };

  const sendKey = (key: (typeof TERM_KEY_BAR)[number]["key"]) => {
    if (active === null) return;
    const appCursor =
      viewsRef.current.get(active)?.term.modes.applicationCursorKeysMode ?? false;
    socketRef.current?.send(
      JSON.stringify({
        type: "input",
        pane: active,
        data: termKeySequence(key, appCursor),
      }),
    );
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
    <section className={`flex min-h-0 min-w-0 flex-col border-t border-ink-700 ${className}`}>
      <div className="flex shrink-0 items-center gap-2 bg-ink-900 px-2 py-1">
        <button
          onClick={create}
          title="New terminal"
          aria-label="New terminal"
          className="ml-auto flex shrink-0 items-center rounded-sm px-1.5 py-0.5 text-ink-400 hover:text-accent"
        >
          <PlusIcon />
        </button>
        <button
          onClick={onToggleMaximized}
          aria-pressed={maximized}
          title={maximized ? "Restore panel height" : "Maximize the panel"}
          aria-label={maximized ? "Restore panel height" : "Maximize the panel"}
          className="hidden shrink-0 items-center rounded-sm px-1.5 py-0.5 text-ink-400 hover:text-accent md:flex"
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
      {panes.length > 0 && (
        <div className="flex shrink-0 items-stretch gap-1 overflow-x-auto border-t border-ink-700 bg-ink-900 px-1 py-1 md:hidden">
          {TERM_KEY_BAR.map(({ key, label, aria }) => (
            <button
              key={key}
              onPointerDown={(event) => event.preventDefault()}
              onClick={() => sendKey(key)}
              aria-label={aria}
              className="flex min-h-9 min-w-9 shrink-0 items-center justify-center rounded-sm border border-ink-700 bg-ink-850 px-2 text-xs text-ink-200 active:bg-ink-700 active:text-accent"
            >
              {label}
            </button>
          ))}
        </div>
      )}
    </section>
  );
}
