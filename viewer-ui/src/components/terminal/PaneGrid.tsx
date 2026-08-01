import type { CSSProperties, MutableRefObject } from "react";
import type { CellPlacement } from "../../lib/terminalLayout";
import type { RecoveryByPane } from "../../lib/recovery";
import { TerminalCell } from "./TerminalCell";
import { StartupSlots } from "./StartupSlots";

export interface PaneGridProps {
  /** The element the panel measures to size its panes. */
  containerRef: React.RefObject<HTMLDivElement | null>;
  panes: number[];
  titles: Record<number, string>;
  active: number | null;
  /** The pane filling the panel, or null for the grid. */
  zoom: number | null;
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

/** Every pane the panel shows, in the cells `layout` gives them. */
export function PaneGrid({
  containerRef,
  panes,
  titles,
  active,
  zoom,
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
  return (
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
        );
      })}
    </div>
  );
}
