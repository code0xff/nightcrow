import type { CSSProperties, MutableRefObject } from "react";
import type { CellPlacement } from "../../lib/terminalLayout";
import type { RecoveryByPane } from "../../lib/recovery";
import { stackedCellStyle, type PaneViewMode } from "../../lib/paneViewMode";
import { TerminalCell } from "./TerminalCell";
import { StartupSlots } from "./StartupSlots";

export interface PaneGridProps {
  /** The element the panel measures to size its panes. */
  containerRef: React.RefObject<HTMLDivElement | null>;
  mode: PaneViewMode;
  panes: number[];
  titles: Record<number, string>;
  active: number | null;
  /** In tabs mode the pane on screen; in grid mode the pane filling the panel,
   *  or null for the grid itself. */
  shown: number | null;
  layout: { cols: number; rows: number; cells: CellPlacement[] };
  /** How many startup terminals are waiting to be measured, or null. */
  pending: number | null;
  recovery: RecoveryByPane;
  draggingPane: number | null;
  dragOverPane: number | null;
  reorderable: boolean;
  slotRefs: MutableRefObject<Map<number, HTMLDivElement>>;
  bodyRefs: MutableRefObject<Map<number, HTMLDivElement>>;
  onFocus: (pane: number) => void;
  onToggleZoom: (pane: number) => void;
  onClose: (pane: number) => void;
  onCancelRecovery: (pane: number) => void;
  onPaneDragStart: (e: React.PointerEvent, pane: number) => void;
  onPaneDragMove: (e: React.PointerEvent) => void;
  onPaneDragEnd: () => void;
  onPaneDragCancel: () => void;
}

/**
 * Every pane the panel holds: side by side in the cells `layout` gives them, or
 * stacked so a tab strip can bring one forward.
 *
 * Both arrangements render every pane. A pane the panel is not showing is still
 * a running program whose output must land somewhere, and in tabs mode it also
 * keeps the size it will be shown at — see `stackedCellStyle`.
 */
export function PaneGrid({
  containerRef,
  mode,
  panes,
  titles,
  active,
  shown,
  layout,
  pending,
  recovery,
  draggingPane,
  dragOverPane,
  reorderable,
  slotRefs,
  bodyRefs,
  onFocus,
  onToggleZoom,
  onClose,
  onCancelRecovery,
  onPaneDragStart,
  onPaneDragMove,
  onPaneDragEnd,
  onPaneDragCancel,
}: PaneGridProps) {
  const tabs = mode === "tabs";
  const placedStyle = (index: number): CSSProperties => {
    const cell = layout.cells[index];
    return {
      display: "flex",
      gridColumn: `${cell.colStart} / span ${cell.colSpan}`,
      gridRow: `${cell.row}`,
    };
  };
  const cellStyle = (pane: number, index: number): CSSProperties => {
    if (tabs) return stackedCellStyle(pane === shown);
    if (shown !== null) return { display: pane === shown ? "flex" : "none" };
    return placedStyle(index);
  };

  return (
    <div
      ref={containerRef}
      className={tabs ? "relative h-full" : "grid h-full gap-1"}
      style={
        tabs
          ? undefined
          : shown !== null
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
          showHeader={!tabs}
          // The first slot stands for the tab that will be on screen; the rest
          // are measured behind it, at the same size.
          slotStyle={(slot) =>
            tabs ? stackedCellStyle(slot === 0) : placedStyle(slot)
          }
          slotRefs={slotRefs}
        />
      )}
      {panes.map((pane, index) => (
        <TerminalCell
          key={pane}
          pane={pane}
          index={index}
          label={titles[pane] ?? `term ${index + 1}`}
          cellStyle={cellStyle(pane, index)}
          isActive={pane === active}
          isZoomed={!tabs && shown === pane}
          showZoom={!tabs && panes.length > 1}
          isDragged={draggingPane === pane}
          isDropTarget={dragOverPane === pane}
          reorderable={reorderable}
          showHeader={!tabs}
          recovery={recovery[pane]}
          onCancelRecovery={() => onCancelRecovery(pane)}
          onFocus={() => onFocus(pane)}
          onToggleZoom={() => onToggleZoom(pane)}
          onClose={() => onClose(pane)}
          onPaneDragStart={(e) => onPaneDragStart(e, pane)}
          onPaneDragMove={onPaneDragMove}
          onPaneDragEnd={onPaneDragEnd}
          onPaneDragCancel={onPaneDragCancel}
          bodyRef={(node) => {
            if (node) bodyRefs.current.set(pane, node);
            else bodyRefs.current.delete(pane);
          }}
        />
      ))}
    </div>
  );
}
