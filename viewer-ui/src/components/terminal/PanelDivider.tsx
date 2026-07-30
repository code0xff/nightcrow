import type { PointerEvent as ReactPointerEvent } from "react";

export interface PanelDividerProps {
  /** False when the split percentage is not what sizes the panels: a maximized
   *  panel has literal grid tracks, and below `md` one view fills the screen. */
  showDivider: boolean;
  draggingUpper: boolean;
  onUpperDragStart: (e: ReactPointerEvent) => void;
  onUpperDragMove: (e: ReactPointerEvent) => void;
  onUpperDragEnd: () => void;
  onUpperDragCancel: () => void;
}

/**
 * The handle that resizes the terminal panel, on the border it shares with the
 * diff panel above.
 *
 * Rendered inside the terminal panel and positioned over that border, rather
 * than as a element of its own in the app grid: that grid places its four
 * children by source order at every breakpoint (see `appLayout.ts`), so a fifth
 * would shift them into the wrong tracks.
 */
export function PanelDivider({
  showDivider,
  draggingUpper,
  onUpperDragStart,
  onUpperDragMove,
  onUpperDragEnd,
  onUpperDragCancel,
}: PanelDividerProps) {
  if (!showDivider) return null;
  return (
    <div
      role="separator"
      aria-orientation="horizontal"
      aria-label="Resize the terminal panel (double-click to reset)"
      title="Drag to resize · double-click to reset"
      onPointerDown={onUpperDragStart}
      onPointerMove={onUpperDragMove}
      onPointerUp={onUpperDragEnd}
      onPointerCancel={onUpperDragCancel}
      // Lost capture ends the drag the same way a release does: the pointer is
      // gone and whatever it last chose is the answer.
      onLostPointerCapture={onUpperDragEnd}
      className={`absolute -top-px left-0 z-10 hidden h-1.5 w-full cursor-row-resize touch-none md:block ${
        draggingUpper ? "bg-accent" : "hover:bg-accent"
      }`}
    />
  );
}
