import { useRef, useState, type PointerEvent as ReactPointerEvent } from "react";
import { reorderByDrop } from "../paneOrder";

const DRAG_THRESHOLD_PX = 4;

interface UseRepoDragArgs {
  ids: string[];
  onReorder: (order: string[]) => void;
  draggingRef: React.MutableRefObject<boolean>;
}

export function useRepoDrag({ ids, onReorder, draggingRef }: UseRepoDragArgs) {
  const draggedRef = useRef<string | null>(null);
  const startRef = useRef<{ x: number; y: number } | null>(null);
  const targetRef = useRef<string | null>(null);
  const [dragging, setDragging] = useState<string | null>(null);
  const [target, setTarget] = useState<string | null>(null);

  const end = () => {
    const dragged = draggedRef.current;
    const dropTarget = targetRef.current;
    if (dragged !== null && draggingRef.current && dropTarget !== null) {
      onReorder(reorderByDrop(ids, dragged, dropTarget));
    }
    draggedRef.current = null;
    startRef.current = null;
    targetRef.current = null;
    draggingRef.current = false;
    setDragging(null);
    setTarget(null);
  };

  const onStart = (event: ReactPointerEvent, id: string) => {
    if ((event.target as HTMLElement).closest("button[data-tab-close]")) return;
    if (event.button !== 0 || ids.length < 2) return;
    draggedRef.current = id;
    startRef.current = { x: event.clientX, y: event.clientY };
    draggingRef.current = false;
  };

  const onMove = (event: ReactPointerEvent) => {
    const dragged = draggedRef.current;
    const start = startRef.current;
    if (dragged === null || start === null) return;
    if (!draggingRef.current && event.buttons === 0) {
      draggedRef.current = null;
      startRef.current = null;
      return;
    }
    if (
      !draggingRef.current &&
      Math.hypot(event.clientX - start.x, event.clientY - start.y) <
        DRAG_THRESHOLD_PX
    ) {
      return;
    }
    if (!draggingRef.current) event.currentTarget.setPointerCapture(event.pointerId);
    draggingRef.current = true;
    setDragging(dragged);
    const element = document
      .elementFromPoint(event.clientX, event.clientY)
      ?.closest("[data-repo-id]");
    const over = element?.getAttribute("data-repo-id") ?? null;
    const nextTarget = over !== dragged ? over : null;
    targetRef.current = nextTarget;
    setTarget(nextTarget);
  };

  return {
    dragging,
    target,
    onStart,
    onMove,
    onEnd: end,
  };
}
