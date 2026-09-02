import { useCallback, useRef } from "react";
import type { PointerEvent as ReactPointerEvent } from "react";
import { useDividerDrag } from "./ui/useDividerDrag";

export interface UseSidebarDragArgs {
  sidebarRef: React.RefObject<HTMLElement | null>;
  sidebarWidth: number;
  resizeSidebar: (px: number) => void;
  commitSidebarWidth: (px: number) => void;
  resetSidebarWidth: () => void;
}

export interface UseSidebarDragResult {
  draggingSidebar: boolean;
  onSidebarDragStart: (e: ReactPointerEvent) => void;
  onSidebarDragMove: (e: ReactPointerEvent) => void;
  onSidebarDragEnd: () => void;
  onSidebarDragCancel: () => void;
}

/**
 * The file sidebar's width, dragged from the divider at its right edge.
 *
 * The gesture itself is [`useDividerDrag`]; what belongs here is the
 * measurement — an absolute width from the sidebar's left edge, captured once
 * at the start so a mid-drag re-layout cannot move the origin under the
 * pointer. The raw distance is passed on unclamped, because the width setters
 * own the bounds.
 */
export function useSidebarDrag({
  sidebarRef,
  sidebarWidth,
  resizeSidebar,
  commitSidebarWidth,
  resetSidebarWidth,
}: UseSidebarDragArgs): UseSidebarDragResult {
  const dragOriginRef = useRef(0);

  const onGestureStart = useCallback(() => {
    const left = sidebarRef.current?.getBoundingClientRect().left;
    if (left === undefined) return false;
    dragOriginRef.current = left;
    return true;
  }, [sidebarRef]);

  const valueAt = useCallback(
    (e: ReactPointerEvent) => e.clientX - dragOriginRef.current,
    [],
  );

  const { dragging, onDragStart, onDragMove, onDragEnd, onDragCancel } =
    useDividerDrag({
    value: sidebarWidth,
    valueAt,
    onGestureStart,
    resize: resizeSidebar,
    commit: commitSidebarWidth,
    reset: resetSidebarWidth,
    axis: "x",
  });

  return {
    draggingSidebar: dragging,
    onSidebarDragStart: onDragStart,
    onSidebarDragMove: onDragMove,
    onSidebarDragEnd: onDragEnd,
    onSidebarDragCancel: onDragCancel,
  };
}
