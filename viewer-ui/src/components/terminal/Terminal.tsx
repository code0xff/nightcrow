import {
  useCallback,
  useEffect,
  useRef,
  useState,
  type CSSProperties,
} from "react";
import { planLayout, type PaneView } from "../../lib/terminalLayout";
import { usePaneDrag } from "../../hooks/terminal/usePaneDrag";
import { useTerminalSocket } from "../../hooks/terminal/useTerminalSocket";
import { useTerminalViews } from "../../hooks/terminal/useTerminalViews";
import { usePaneSizes } from "../../hooks/terminal/usePaneSizes";
import { useStartupSizes } from "../../hooks/terminal/useStartupSizes";
import { TerminalCell } from "./TerminalCell";
import { StartupSlots } from "./StartupSlots";
import { TermKeyBar } from "./TermKeyBar";
import { PanelDivider, type PanelDividerProps } from "./PanelDivider";
import { PanelToolbar } from "./PanelToolbar";
import { TERM_KEY_BAR, termKeySequence } from "../../lib/termKeys";

export function TerminalPanel({
  repo,
  maximized,
  onToggleMaximized,
  className = "",
  sectionRef,
  ...divider
}: {
  repo: string;
  maximized: boolean;
  onToggleMaximized: () => void;
  className?: string;
  /** The panel's own element, the bottom edge of the region the split divides. */
  sectionRef: React.RefObject<HTMLElement | null>;
} & PanelDividerProps) {
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
  // Cells rendered for startup terminals the server has not created yet, so
  // their size can be measured from the slot each will occupy.
  const slotRefs = useRef(new Map<number, HTMLDivElement>());
  const [pending, setPending] = useState<number | null>(null);
  const [panes, setPanes] = useState<number[]>([]);
  const [active, setActive] = useState<number | null>(null);
  const [zoomed, setZoomed] = useState<number | null>(null);
  const [size, setSize] = useState({ w: 0, h: 0 });
  const [titles, setTitles] = useState<Record<number, string>>({});
  // Whether this page's layout is what sets the pane sizes. A PTY has one size
  // and the child cannot be re-flowed afterwards, so one client at a time
  // decides it; the rest render the grid they are given.
  const [ownsSize, setOwnsSize] = useState(true);

  useTerminalSocket({
    repo,
    socketRef,
    viewsRef,
    pendingRef,
    sentSizesRef,
    lastActiveByRepoRef,
    expectCreateRef,
    setPending,
    setPanes,
    setActive,
    setZoomed,
    setTitles,
    setOwnsSize,
  });

  useTerminalViews({
    panes,
    size,
    zoomed,
    socketRef,
    viewsRef,
    bodyRefs,
    pendingRef,
    setTitles,
  });

  const onAnswered = useCallback(() => setPending(null), []);
  useStartupSizes({
    pending,
    size,
    socketRef,
    slotRefs,
    panesExist: panes.length > 0,
    onAnswered,
  });

  usePaneSizes({
    panes,
    size,
    zoomed,
    socketRef,
    viewsRef,
    bodyRefs,
    sentSizesRef,
    ownsSize,
  });

  useEffect(() => {
    const container = containerRef.current;
    if (!container) return;
    // Keep the same object when the pixels are unchanged. A fresh one is never
    // `Object.is`-equal, so React would re-render — and every consumer would
    // re-fit every pane — for observer callbacks that carry no news, which the
    // browser delivers whenever anything in the subtree relayouts.
    const observer = new ResizeObserver(() => {
      const w = container.clientWidth;
      const h = container.clientHeight;
      setSize((current) =>
        current.w === w && current.h === h ? current : { w, h },
      );
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

  // Take the sizing back. Deliberate rather than automatic: the panes belong to
  // a session someone else may be working in, and merely opening this page must
  // not repaint their screen.
  const claimSize = () => {
    socketRef.current?.send(JSON.stringify({ type: "claim_size" }));
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

  // Before the startup terminals exist the grid is planned for the slots they
  // will occupy, so what is measured is the cell each pane actually gets.
  const slots = panes.length > 0 ? panes.length : (pending ?? 0);
  const layout = planLayout(slots, size.w >= size.h);

  return (
    <section
      ref={sectionRef}
      className={`relative flex min-h-0 min-w-0 flex-col border-t border-ink-700 ${className}`}
    >
      <PanelDivider {...divider} />
      <PanelToolbar
        ownsSize={ownsSize}
        maximized={maximized}
        onClaimSize={claimSize}
        onCreate={create}
        onToggleMaximized={onToggleMaximized}
      />
      <div className="relative min-h-0 flex-1 overflow-hidden bg-ink-950 p-1">
        {panes.length === 0 && pending === null && (
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
          {panes.length === 0 && pending !== null && (
            <StartupSlots
              count={pending}
              cells={layout.cells}
              slotRefs={slotRefs}
            />
          )}
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
                showZoom={panes.length > 1}
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
      {panes.length > 0 && <TermKeyBar onKey={sendKey} />}
    </section>
  );
}
