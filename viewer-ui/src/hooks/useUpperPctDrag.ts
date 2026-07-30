import { useCallback, useRef } from "react";
import type { PointerEvent as ReactPointerEvent } from "react";
import { useDividerDrag } from "./ui/useDividerDrag";
import { upperPctAt } from "../lib/upperPct";

export interface UseUpperPctDragArgs {
  /** The diff panel — the split region's top edge. */
  upperRef: React.RefObject<HTMLElement | null>;
  /** The terminal panel — the split region's bottom edge. */
  lowerRef: React.RefObject<HTMLElement | null>;
  upperPct: number;
  resizeUpperPct: (pct: number) => void;
  commitUpperPct: (pct: number) => void;
  resetUpperPct: () => void;
  // Prevent an older poll from overwriting the split during a drag.
  bumpUpperPctWrites: () => void;
}

export interface UseUpperPctDragResult {
  draggingUpper: boolean;
  onUpperDragStart: (e: ReactPointerEvent) => void;
  onUpperDragMove: (e: ReactPointerEvent) => void;
  onUpperDragEnd: () => void;
  onUpperDragCancel: () => void;
  upperDraggingRef: React.MutableRefObject<boolean>;
}

/**
 * The split between the diff panel and the terminal panel, dragged from the
 * divider on the border between them.
 *
 * The gesture itself is [`useDividerDrag`]; what belongs here is the
 * measurement. Two refs rather than one because a percentage needs both edges
 * of the region, and the region is two grid tracks with no element of its own —
 * the top comes from the diff panel, the bottom from the terminal panel. Both
 * are read once at the start, so the dragging itself (which moves both) cannot
 * shift the frame it is being measured against.
 */
export function useUpperPctDrag({
  upperRef,
  lowerRef,
  upperPct,
  resizeUpperPct,
  commitUpperPct,
  resetUpperPct,
  bumpUpperPctWrites,
}: UseUpperPctDragArgs): UseUpperPctDragResult {
  const topRef = useRef(0);
  const bottomRef = useRef(0);

  const onGestureStart = useCallback(() => {
    const top = upperRef.current?.getBoundingClientRect().top;
    const bottom = lowerRef.current?.getBoundingClientRect().bottom;
    if (top === undefined || bottom === undefined) return false;
    topRef.current = top;
    bottomRef.current = bottom;
    bumpUpperPctWrites();
    return true;
  }, [upperRef, lowerRef, bumpUpperPctWrites]);

  const valueAt = useCallback(
    (e: ReactPointerEvent) =>
      upperPctAt(e.clientY, topRef.current, bottomRef.current, upperPct),
    [upperPct],
  );

  const {
    dragging,
    onDragStart,
    onDragMove,
    onDragEnd,
    onDragCancel,
    draggingRef,
  } = useDividerDrag({
    value: upperPct,
    valueAt,
    onGestureStart,
    resize: resizeUpperPct,
    commit: commitUpperPct,
    reset: resetUpperPct,
    axis: "y",
  });

  return {
    draggingUpper: dragging,
    onUpperDragStart: onDragStart,
    onUpperDragMove: onDragMove,
    onUpperDragEnd: onDragEnd,
    onUpperDragCancel: onDragCancel,
    upperDraggingRef: draggingRef,
  };
}
