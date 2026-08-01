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
import { usePaneRecovery } from "../../hooks/terminal/usePaneRecovery";
import { usePaneCommands } from "../../hooks/terminal/usePaneCommands";
import { useStartupSizes } from "../../hooks/terminal/useStartupSizes";
import { TerminalCell } from "./TerminalCell";
import { StartupSlots } from "./StartupSlots";
import { TermKeyBar } from "./TermKeyBar";
import { PanelDivider, type PanelDividerProps } from "./PanelDivider";
import { PanelToolbar } from "./PanelToolbar";
import { renderedZoom, zoomPending } from "../../lib/zoom";

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
  // A zoom this page has asked for and not yet been answered. Held here because
  // both halves need it: the commands read it, the socket clears it.
  const zoomAskedRef = useRef<number | null | undefined>(undefined);
  // Cells rendered for startup terminals the server has not created yet, so
  // their size can be measured from the slot each will occupy.
  const slotRefs = useRef(new Map<number, HTMLDivElement>());
  const [pending, setPending] = useState<number | null>(null);
  // Panes the replay has promised but not yet delivered. The grid is planned for
  // them too, so each pane arrives into the cell it will keep instead of being
  // given the whole panel and shrunk by the next one.
  const [replayLeft, setReplayLeft] = useState(0);
  const [panes, setPanes] = useState<number[]>([]);
  const [active, setActive] = useState<number | null>(null);
  const [zoomed, setZoomed] = useState<number | null>(null);
  const [size, setSize] = useState({ w: 0, h: 0 });
  const [titles, setTitles] = useState<Record<number, string>>({});
  // Whether this page's layout is what sets the pane sizes. A PTY has one size
  // and the child cannot be re-flowed afterwards, so one client at a time
  // decides it; the rest render the grid they are given.
  const [ownsSize, setOwnsSize] = useState(true);
  const { recovery, setRecovery, cancelRecovery } = usePaneRecovery(socketRef);
  // Derived rather than corrected in the handler, so the panel cannot render a
  // state its pane list does not support at all. See `lib/zoom.ts`.
  const zoom = renderedZoom(zoomed, panes);

  useTerminalSocket({
    repo,
    socketRef,
    viewsRef,
    pendingRef,
    sentSizesRef,
    lastActiveByRepoRef,
    zoomAskedRef,
    setPending,
    setReplayLeft,
    setPanes,
    setActive,
    setZoomed,
    setTitles,
    setOwnsSize,
    setRecovery,
  });

  useTerminalViews({
    panes,
    size,
    zoomed: zoom,
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
    zoomed: zoom,
    socketRef,
    viewsRef,
    bodyRefs,
    sentSizesRef,
    ownsSize,
    layoutPending: zoomPending(zoomed, panes),
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
    // Not while a replay has named a zoom whose pane has not arrived: the panes
    // come one at a time, and focusing an earlier one would put the keyboard in
    // a terminal that is about to be replaced by the zoomed one.
    if (zoomPending(zoomed, panes)) return;
    if (active === null && panes.length > 0) {
      const remembered = lastActiveByRepoRef.current.get(repo);
      setActive(
        remembered !== undefined && panes.includes(remembered)
          ? remembered
          : panes[panes.length - 1],
      );
    }
  }, [active, panes, repo, zoomed]);

  // While one pane fills the panel it is the only one that can be seen, so it
  // has to be the one being typed into. Enforced here rather than at the toggle
  // because a zoom no longer needs a click on this page to happen: it is
  // replayed on connect and set by other clients, and either would otherwise
  // leave the keyboard — and the key bar, which types into the active pane —
  // pointed at a terminal that is not on screen.
  useEffect(() => {
    if (zoom !== null && zoom !== active) {
      setActive(zoom);
      lastActiveByRepoRef.current.set(repo, zoom);
    }
  }, [zoom, active, repo]);

  useEffect(() => {
    if (active !== null) viewsRef.current.get(active)?.term.focus();
  }, [active]);

  const focusPane = (pane: number) => {
    setActive(pane);
    lastActiveByRepoRef.current.set(repo, pane);
  };

  const { create, toggleZoom, claimSize, closePane, reorder, sendKey } =
    usePaneCommands({
      socketRef,
      viewsRef,
      zoomed: zoom,
      zoomAskedRef,
      active,
    });

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
    zoomed: zoom,
    onFocus: focusPane,
    onReorder: reorder,
  });

  // Before the startup terminals exist the grid is planned for the slots they
  // will occupy, so what is measured is the cell each pane actually gets. The
  // same for a replay in progress: its remaining panes hold their cells open,
  // which is what keeps the ones already here from being laid out twice.
  const slots =
    panes.length + replayLeft > 0 ? panes.length + replayLeft : (pending ?? 0);
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
        recovery={recovery}
        panes={panes}
        onCancelRecovery={cancelRecovery}
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
            zoom !== null
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
              zoom !== null
                ? { display: pane === zoom ? "flex" : "none" }
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
                isZoomed={zoom === pane}
                showZoom={panes.length > 1}
                isDragged={draggingPane === pane}
                isDropTarget={dragOverPane === pane}
                reorderable={reorderable}
                recovery={recovery[pane]}
                onCancelRecovery={() => cancelRecovery(pane)}
                onFocus={() => focusPane(pane)}
                onToggleZoom={() => toggleZoom(pane)}
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
