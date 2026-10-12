import { useRef, useState } from "react";
import { reorderByDrop } from "../../lib/terminal/paneOrder";
import {
  PANE_DRAG_HOLD_MS,
  PANE_DRAG_THRESHOLD_PX,
} from "../../lib/terminal/terminalLayout";

interface UsePaneDragArgs {
  panes: number[];
  zoomed: number | null;
  onFocus: (pane: number) => void;
  onReorder: (order: number[]) => void;
}

export interface PaneDragStartOptions {
  /**
   * The surface scrolls under a touch, so a finger has to rest on it before it
   * picks the pane up. Until then the gesture is the browser's: a swipe scrolls
   * and arrives here as a cancel, and a tap focuses on release rather than on
   * contact, so scrolling past a tab does not switch to it.
   */
  holdToDrag?: boolean;
}

/// Refs keep pointerup from observing stale drag state.
export function usePaneDrag({ panes, zoomed, onFocus, onReorder }: UsePaneDragArgs) {
  const dragPaneRef = useRef<number | null>(null);
  const dragStartRef = useRef<{ x: number; y: number } | null>(null);
  const dragOverRef = useRef<number | null>(null);
  const draggingRef = useRef(false);
  const holdTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  // Set while a touch on a hold-to-drag surface has not been released yet.
  const tapPendingRef = useRef(false);
  const [draggingPane, setDraggingPane] = useState<number | null>(null);
  const [dragOverPane, setDragOverPane] = useState<number | null>(null);

  const reorderable = zoomed === null && panes.length > 1;

  const clearHold = () => {
    if (holdTimerRef.current !== null) clearTimeout(holdTimerRef.current);
    holdTimerRef.current = null;
  };

  const endPaneDrag = () => {
    clearHold();
    tapPendingRef.current = false;
    dragPaneRef.current = null;
    dragStartRef.current = null;
    dragOverRef.current = null;
    draggingRef.current = false;
    setDraggingPane(null);
    setDragOverPane(null);
  };

  const onPaneDragStart = (
    e: React.PointerEvent,
    pane: number,
    options?: PaneDragStartOptions,
  ) => {
    if ((e.target as HTMLElement).closest("button")) return;
    const hold = options?.holdToDrag === true && e.pointerType === "touch";
    if (!hold) onFocus(pane);
    if (e.button !== 0 || (!reorderable && !hold)) return;
    dragPaneRef.current = pane;
    dragStartRef.current = { x: e.clientX, y: e.clientY };
    draggingRef.current = false;
    if (!hold) {
      e.currentTarget.setPointerCapture(e.pointerId);
      return;
    }
    tapPendingRef.current = true;
    if (!reorderable) return;
    const surface = e.currentTarget;
    const pointerId = e.pointerId;
    clearHold();
    holdTimerRef.current = setTimeout(() => {
      holdTimerRef.current = null;
      tapPendingRef.current = false;
      draggingRef.current = true;
      setDraggingPane(pane);
      // The browser may already have let go of the pointer; the drag then
      // ends on the cancel it sent, and there is nothing to capture.
      try {
        surface.setPointerCapture(pointerId);
      } catch {
        endPaneDrag();
      }
    }, PANE_DRAG_HOLD_MS);
  };

  const onPaneDragMove = (e: React.PointerEvent) => {
    const dragged = dragPaneRef.current;
    const start = dragStartRef.current;
    if (dragged === null || start === null) return;
    if (!draggingRef.current) {
      const travelled =
        Math.hypot(e.clientX - start.x, e.clientY - start.y) >=
        PANE_DRAG_THRESHOLD_PX;
      if (!travelled) return;
      // A finger that moved before the hold ran out is scrolling, not
      // dragging and not tapping.
      if (tapPendingRef.current) {
        endPaneDrag();
        return;
      }
    }
    draggingRef.current = true;
    setDraggingPane(dragged);
    const el = document
      .elementFromPoint(e.clientX, e.clientY)
      ?.closest("[data-pane-id]");
    const over = el ? Number(el.getAttribute("data-pane-id")) : null;
    const target = over !== null && over !== dragged ? over : null;
    dragOverRef.current = target;
    setDragOverPane(target);
  };

  const onPaneDragEnd = () => {
    const dragged = dragPaneRef.current;
    const target = dragOverRef.current;
    if (dragged !== null && tapPendingRef.current) onFocus(dragged);
    if (dragged !== null && draggingRef.current && target !== null) {
      onReorder(reorderByDrop(panes, dragged, target));
    }
    endPaneDrag();
  };

  return {
    draggingPane,
    dragOverPane,
    reorderable,
    endPaneDrag,
    onPaneDragStart,
    onPaneDragMove,
    onPaneDragEnd,
  };
}
