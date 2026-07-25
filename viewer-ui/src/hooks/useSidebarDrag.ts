import { useCallback, useRef, useState } from "react";
import type { PointerEvent as ReactPointerEvent } from "react";

/// Prevent clicks and vertical jitter from committing the viewport-capped width.
const SIDEBAR_DRAG_THRESHOLD_PX = 3;

const DOUBLE_CLICK_MS = 400;

export interface UseSidebarDragArgs {
  sidebarRef: React.RefObject<HTMLElement | null>;
  sidebarWidth: number;
  resizeSidebar: (px: number) => void;
  commitSidebarWidth: (px: number) => void;
  resetSidebarWidth: () => void;
  // Prevent an older poll from overwriting the width during a drag.
  bumpSidebarWrites: () => void;
}

export interface UseSidebarDragResult {
  draggingSidebar: boolean;
  onSidebarDragStart: (e: ReactPointerEvent) => void;
  onSidebarDragMove: (e: ReactPointerEvent) => void;
  onSidebarDragEnd: () => void;
  onSidebarDragCancel: () => void;
  draggingRef: React.MutableRefObject<boolean>;
}

/** Capture the origin so a mid-drag re-layout cannot move it under the pointer. */
export function useSidebarDrag({
  sidebarRef,
  sidebarWidth,
  resizeSidebar,
  commitSidebarWidth,
  resetSidebarWidth,
  bumpSidebarWrites,
}: UseSidebarDragArgs): UseSidebarDragResult {
  const dragOriginRef = useRef(0);
  const dragStartXRef = useRef(0);
  const dragWidthRef = useRef(0);
  // State drives visuals; the ref makes move/end handling synchronous and once-only.
  const draggingRef = useRef(false);
  // A bare click must not persist the viewport-capped display width.
  const dragMovedRef = useRef(false);
  // Track no-move releases because pointerdown preventDefault can suppress dblclick.
  const lastClickRef = useRef(0);
  const [draggingSidebar, setDraggingSidebar] = useState(false);
  const onSidebarDragStart = useCallback(
    (e: ReactPointerEvent) => {
      if (e.button !== 0 || !e.isPrimary) return;
      const left = sidebarRef.current?.getBoundingClientRect().left;
      if (left === undefined) return;
      dragOriginRef.current = left;
      dragStartXRef.current = e.clientX;
      dragWidthRef.current = sidebarWidth;
      draggingRef.current = true;
      dragMovedRef.current = false;
      bumpSidebarWrites();
      setDraggingSidebar(true);
      e.currentTarget.setPointerCapture(e.pointerId);
      e.preventDefault();
    },
    [sidebarWidth, sidebarRef, bumpSidebarWrites],
  );
  const onSidebarDragMove = useCallback(
    (e: ReactPointerEvent) => {
      if (!draggingRef.current) return;
      if (
        !dragMovedRef.current &&
        Math.abs(e.clientX - dragStartXRef.current) < SIDEBAR_DRAG_THRESHOLD_PX
      ) {
        return;
      }
      dragMovedRef.current = true;
      dragWidthRef.current = e.clientX - dragOriginRef.current;
      resizeSidebar(dragWidthRef.current);
    },
    [resizeSidebar],
  );
  // Pointerup and lost-capture share this once-only end path.
  const onSidebarDragEnd = useCallback(() => {
    if (!draggingRef.current) return;
    draggingRef.current = false;
    setDraggingSidebar(false);
    if (dragMovedRef.current) {
      commitSidebarWidth(dragWidthRef.current);
      lastClickRef.current = 0;
      return;
    }
    const now = Date.now();
    if (now - lastClickRef.current < DOUBLE_CLICK_MS) {
      lastClickRef.current = 0;
      resetSidebarWidth();
    } else {
      lastClickRef.current = now;
    }
  }, [commitSidebarWidth, resetSidebarWidth]);
  // Cancelled gestures must not persist their partial position.
  const onSidebarDragCancel = useCallback(() => {
    draggingRef.current = false;
    dragMovedRef.current = false;
    lastClickRef.current = 0;
    setDraggingSidebar(false);
  }, []);
  return {
    draggingSidebar,
    onSidebarDragStart,
    onSidebarDragMove,
    onSidebarDragEnd,
    onSidebarDragCancel,
    draggingRef,
  };
}
