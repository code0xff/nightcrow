import type { MutableRefObject } from "react";
import type { CellPlacement } from "../../lib/terminalLayout";
import { TerminalCell } from "./TerminalCell";

/**
 * The cells startup terminals will occupy, rendered before those terminals
 * exist so their size can be measured (see `useStartupSizes`).
 *
 * A real `TerminalCell` rather than a bare box, chrome and all: a placeholder
 * that skipped the header would measure a taller body than the pane it stands
 * in for, and the PTY would be born at a size no pane ever has — which is the
 * whole thing the handshake removes.
 */
export function StartupSlots({
  count,
  cells,
  slotRefs,
}: {
  count: number;
  cells: CellPlacement[];
  slotRefs: MutableRefObject<Map<number, HTMLDivElement>>;
}) {
  return Array.from({ length: count }, (_, slot) => {
    const cell = cells[slot];
    return (
      <TerminalCell
        key={`slot-${slot}`}
        // Negative so it can never collide with a real pane id.
        pane={-1 - slot}
        index={slot}
        label="starting…"
        cellStyle={{
          display: "flex",
          gridColumn: `${cell.colStart} / span ${cell.colSpan}`,
          gridRow: `${cell.row}`,
        }}
        isActive={false}
        isZoomed={false}
        showZoom={false}
        isDragged={false}
        isDropTarget={false}
        reorderable={false}
        onCancelRecovery={() => {}}
        onFocus={() => {}}
        onToggleZoom={() => {}}
        onClose={() => {}}
        onPaneDragStart={() => {}}
        onPaneDragMove={() => {}}
        onPaneDragEnd={() => {}}
        onPaneDragCancel={() => {}}
        bodyRef={(node) => {
          if (node) slotRefs.current.set(slot, node);
          else slotRefs.current.delete(slot);
        }}
      />
    );
  });
}
