import { useCallback, useRef, useState } from "react";
import type { PointerEvent as ReactPointerEvent } from "react";

/// Prevent clicks and cross-axis jitter from committing a display-capped value.
export const DIVIDER_DRAG_THRESHOLD_PX = 3;

const DOUBLE_CLICK_MS = 400;

export interface UseDividerDragArgs {
  /** The value the divider currently sits at, so a release with no movement
   *  has something to commit that the drag did not invent. */
  value: number;
  /** Where the pointer puts the divider. `null` aborts the gesture before it
   *  starts — the geometry it needs is not measurable yet. Called once per
   *  move, and the caller is expected to have captured its origin at
   *  `onGestureStart` time so a mid-drag re-layout cannot move it under the
   *  pointer. */
  valueAt: (e: ReactPointerEvent) => number | null;
  /** Prepare for a gesture: measure whatever `valueAt` will read, and count the
   *  write the release will make. Runs once, at start; `false` aborts. */
  onGestureStart: () => boolean;
  /** Local-only update, once per pointer move. */
  resize: (value: number) => void;
  /** Persist, once, on release — and only if the pointer actually moved. */
  commit: (value: number) => void;
  /** Restore the default, on double click. */
  reset: () => void;
  /** The axis the movement threshold is measured along. */
  axis: "x" | "y";
}

export interface UseDividerDragResult {
  dragging: boolean;
  onDragStart: (e: ReactPointerEvent) => void;
  onDragMove: (e: ReactPointerEvent) => void;
  onDragEnd: () => void;
  onDragCancel: () => void;
  draggingRef: React.MutableRefObject<boolean>;
}

/**
 * The pointer bookkeeping every resize divider needs, with the geometry left
 * to the caller.
 *
 * Both dividers want the same gesture — a movement threshold so a bare click
 * does not commit, local updates while dragging with one write on release, and
 * a double click that restores the default — but they compute different things
 * from the pointer: the sidebar an absolute width from one edge, the panel
 * split a percentage of a region spanning two grid tracks. So the state
 * machine lives here and the measurement stays with whoever knows the layout.
 */
export function useDividerDrag({
  value,
  valueAt,
  onGestureStart,
  resize,
  commit,
  reset,
  axis,
}: UseDividerDragArgs): UseDividerDragResult {
  const dragStartRef = useRef(0);
  const dragValueRef = useRef(0);
  // State drives visuals; the ref makes move/end handling synchronous and once-only.
  const draggingRef = useRef(false);
  // A bare click must not persist a display-capped value.
  const dragMovedRef = useRef(false);
  // Track no-move releases because pointerdown preventDefault can suppress dblclick.
  const lastClickRef = useRef(0);
  const [dragging, setDragging] = useState(false);

  const onDragStart = useCallback(
    (e: ReactPointerEvent) => {
      if (e.button !== 0 || !e.isPrimary) return;
      if (!onGestureStart()) return;
      dragStartRef.current = axis === "x" ? e.clientX : e.clientY;
      dragValueRef.current = value;
      draggingRef.current = true;
      dragMovedRef.current = false;
      setDragging(true);
      e.currentTarget.setPointerCapture(e.pointerId);
      e.preventDefault();
    },
    [value, onGestureStart, axis],
  );

  const onDragMove = useCallback(
    (e: ReactPointerEvent) => {
      if (!draggingRef.current) return;
      const position = axis === "x" ? e.clientX : e.clientY;
      if (
        !dragMovedRef.current &&
        Math.abs(position - dragStartRef.current) < DIVIDER_DRAG_THRESHOLD_PX
      ) {
        return;
      }
      const next = valueAt(e);
      if (next === null) return;
      dragMovedRef.current = true;
      dragValueRef.current = next;
      resize(next);
    },
    [valueAt, resize, axis],
  );

  // Pointerup and lost-capture share this once-only end path.
  const onDragEnd = useCallback(() => {
    if (!draggingRef.current) return;
    draggingRef.current = false;
    setDragging(false);
    if (dragMovedRef.current) {
      commit(dragValueRef.current);
      lastClickRef.current = 0;
      return;
    }
    const now = Date.now();
    if (now - lastClickRef.current < DOUBLE_CLICK_MS) {
      lastClickRef.current = 0;
      reset();
    } else {
      lastClickRef.current = now;
    }
  }, [commit, reset]);

  // Cancelled gestures must not persist their partial position.
  const onDragCancel = useCallback(() => {
    draggingRef.current = false;
    dragMovedRef.current = false;
    lastClickRef.current = 0;
    setDragging(false);
  }, []);

  return {
    dragging,
    onDragStart,
    onDragMove,
    onDragEnd,
    onDragCancel,
    draggingRef,
  };
}
