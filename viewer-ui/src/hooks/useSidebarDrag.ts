import { useCallback, useRef, useState } from "react";
import type { PointerEvent as ReactPointerEvent } from "react";

/// Horizontal travel before a pointer press on the sidebar divider counts as a
/// resize. Below this, a click or a vertical-only wobble commits nothing, so it
/// cannot overwrite the stored width with the viewport-capped display value.
const SIDEBAR_DRAG_THRESHOLD_PX = 3;

/// Window within which two clicks on the divider read as a double-click and
/// reset the sidebar to its default width.
const DOUBLE_CLICK_MS = 400;

export interface UseSidebarDragArgs {
  sidebarRef: React.RefObject<HTMLElement | null>;
  sidebarWidth: number;
  resizeSidebar: (px: number) => void;
  commitSidebarWidth: (px: number) => void;
  resetSidebarWidth: () => void;
  // Bumps App's write counter at drag start so a poll that left before the drag
  // must not adopt the old server width mid-drag and snap the pane out from
  // under the pointer.
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

/** Dragging the divider between the sidebar and the diff pane. The new width
 *  is the pointer's distance from the sidebar's left edge, captured once at
 *  drag start so a mid-drag re-layout cannot move the origin under the pointer. */
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
  // Synchronous drag gate. The state below drives the cursor and overlay, but
  // the move guard and the once-only commit read this ref so neither a
  // Strict-Mode double-invoke nor the duplicate pointerup/lost-capture pair can
  // fire the write twice, and the first move is not lost to a stale state read.
  const draggingRef = useRef(false);
  // Whether the pointer actually moved between down and up. A bare click must
  // not commit: after a window shrink the displayed width is `min(px, 50vw)`
  // while the stored width is still `px`, so committing the click would persist
  // the capped value and quietly overwrite the shared preference.
  const dragMovedRef = useRef(false);
  // Timestamp of the last no-move release, so two quick clicks on the divider
  // read as a double-click and reset the width. Detected here rather than via a
  // native `ondblclick` because the drag's `preventDefault` on pointerdown can
  // suppress the synthesized click/dblclick events.
  const lastClickRef = useRef(0);
  const [draggingSidebar, setDraggingSidebar] = useState(false);
  const onSidebarDragStart = useCallback(
    (e: ReactPointerEvent) => {
      // Primary button / first touch only, matching a native dblclick: a
      // right- or middle-click must not start a drag or arm the reset.
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
      // Ignore movement until the pointer has travelled horizontally: a
      // vertical-only move or touch jitter must not count as a resize, or it
      // would commit the clientX-derived (viewport-capped) width and overwrite
      // the shared absolute preference without the user meaning to.
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
  // Fires on both pointerup and lost capture; the ref gate commits exactly once,
  // and only when the pointer actually moved (a bare click stores nothing).
  const onSidebarDragEnd = useCallback(() => {
    if (!draggingRef.current) return;
    draggingRef.current = false;
    setDraggingSidebar(false);
    if (dragMovedRef.current) {
      commitSidebarWidth(dragWidthRef.current);
      lastClickRef.current = 0;
      return;
    }
    // No move: a second quick click resets the width to the default; a lone
    // click just arms the next one.
    const now = Date.now();
    if (now - lastClickRef.current < DOUBLE_CLICK_MS) {
      lastClickRef.current = 0;
      resetSidebarWidth();
    } else {
      lastClickRef.current = now;
    }
  }, [commitSidebarWidth, resetSidebarWidth]);
  // A cancelled gesture (OS takeover, focus loss — mostly touch/pen) must not
  // persist its partial position. Clear the gate without committing; the
  // following lost-capture event then no-ops, and the next poll reconciles the
  // partial width back to the server's last committed value.
  const onSidebarDragCancel = useCallback(() => {
    draggingRef.current = false;
    dragMovedRef.current = false;
    // Also disarm the double-click: a cancelled gesture is not a completed
    // click, so it must not pair with the next one into a reset.
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