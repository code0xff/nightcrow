import type { CSSProperties, MutableRefObject } from "react";
import { TerminalCell } from "./TerminalCell";

/**
 * The cells startup terminals will occupy, rendered before those terminals
 * exist so their size can be measured (see `useStartupSizes`).
 *
 * A real `TerminalCell` rather than a bare box, chrome and all: a placeholder
 * that skipped the header would measure a taller body than the pane it stands
 * in for, and the PTY would be born at a size no pane ever has — which is the
 * whole thing the handshake removes. `showHeader` travels for the same reason,
 * so a tabbed panel measures the headerless cell it is going to draw.
 */
export function StartupSlots({
  count,
  slotStyle,
  showHeader,
  bodyTouch,
  slotRefs,
}: {
  count: number;
  slotStyle: (slot: number) => CSSProperties;
  showHeader: boolean;
  bodyTouch: React.ComponentProps<typeof TerminalCell>["bodyTouch"];
  slotRefs: MutableRefObject<Map<number, HTMLDivElement>>;
}) {
  return Array.from({ length: count }, (_, slot) => (
    <TerminalCell
      key={`slot-${slot}`}
      // Negative so it can never collide with a real pane id.
      pane={-1 - slot}
      index={slot}
      label="starting…"
      cellStyle={slotStyle(slot)}
      isActive={false}
      isZoomed={false}
      showZoom={false}
      isDragged={false}
      isDropTarget={false}
      reorderable={false}
      showHeader={showHeader}
      bodyTouch={bodyTouch}
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
  ));
}
