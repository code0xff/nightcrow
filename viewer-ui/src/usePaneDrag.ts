import { useRef, useState } from "react";
import { reorderByDrop } from "./paneOrder";
import { PANE_DRAG_THRESHOLD_PX } from "./terminalLayout";

interface UsePaneDragArgs {
  panes: number[];
  zoomed: number | null;
  onFocus: (pane: number) => void;
  onReorder: (order: number[]) => void;
}

/// Pane drag-to-reorder. The id being dragged and the drop target live in refs
/// (read on pointerup, free of stale-closure risk); the mirrored state only
/// drives the drag styling. `draggingRef` flips once the pointer crosses the
/// dead zone, separating a reorder from a plain header click.
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
    // A press on the header's own buttons (zoom, close) is theirs — do not
    // focus or start a drag, matching the pre-drag behaviour where those
    // buttons stopped the focus press from propagating.
    if ((e.target as HTMLElement).closest("button")) return;
    onFocus(pane);
    // Primary button / first touch only, and only when there is a grid to
    // rearrange (more than one pane, not zoomed).
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
    // Which cell is under the pointer. Pointer capture does not change hit
    // testing, so this still finds the pane being hovered, not the dragged one.
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