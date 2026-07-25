import { useRef, useState } from "react";
import { reorderByDrop } from "../../lib/paneOrder";
import { PANE_DRAG_THRESHOLD_PX } from "../../lib/terminalLayout";

interface UsePaneDragArgs {
  panes: number[];
  zoomed: number | null;
  onFocus: (pane: number) => void;
  onReorder: (order: number[]) => void;
}

/// Refs keep pointerup from observing stale drag state.
export function usePaneDrag({ panes, zoomed, onFocus, onReorder }: UsePaneDragArgs) {
  const dragPaneRef = useRef<number | null>(null);
  const dragStartRef = useRef<{ x: number; y: number } | null>(null);
  const dragOverRef = useRef<number | null>(null);
  const draggingRef = useRef(false);
  const [draggingPane, setDraggingPane] = useState<number | null>(null);
  const [dragOverPane, setDragOverPane] = useState<number | null>(null);

  const reorderable = zoomed === null && panes.length > 1;

  const endPaneDrag = () => {
    dragPaneRef.current = null;
    dragStartRef.current = null;
    dragOverRef.current = null;
    draggingRef.current = false;
    setDraggingPane(null);
    setDragOverPane(null);
  };

  const onPaneDragStart = (e: React.PointerEvent, pane: number) => {
    if ((e.target as HTMLElement).closest("button")) return;
    onFocus(pane);
    if (e.button !== 0 || !reorderable) return;
    dragPaneRef.current = pane;
    dragStartRef.current = { x: e.clientX, y: e.clientY };
    draggingRef.current = false;
    e.currentTarget.setPointerCapture(e.pointerId);
  };

  const onPaneDragMove = (e: React.PointerEvent) => {
    const dragged = dragPaneRef.current;
    const start = dragStartRef.current;
    if (dragged === null || start === null) return;
    if (
      !draggingRef.current &&
      Math.hypot(e.clientX - start.x, e.clientY - start.y) <
        PANE_DRAG_THRESHOLD_PX
    ) {
      return;
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
