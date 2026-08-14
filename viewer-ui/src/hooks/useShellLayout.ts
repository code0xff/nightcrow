import { useRef } from "react";
import { useViewerPrefs } from "./useViewerPrefs";
import { useSidebarDrag } from "./useSidebarDrag";
import { useUpperPctDrag } from "./useUpperPctDrag";

/**
 * The two dividers that decide how the shell is divided up, and the stored
 * preferences behind them.
 *
 * One hook because the pieces only make sense together: each divider needs the
 * element it measures against, the local setter it calls while dragging, the
 * persisting one it calls on release, and the write counter that stops a poll
 * from undoing either. Assembled at the call site, that is four wirings per
 * divider for a page that has no other use for any of them — and a page long
 * enough that the wiring crowds out what it actually renders.
 *
 * `shell` is handed straight to `RepoShell`; the rest is what other parts of the
 * page read (the accent for the header, the split for the grid tracks, the
 * two guards for the poll).
 */
export function useShellLayout() {
  const {
    accent,
    next,
    cycle,
    adoptAccent,
    accentWrites,
    sidebarWidth,
    resizeSidebar,
    commitSidebarWidth,
    resetSidebarWidth,
    bumpSidebarWrites,
    adoptSidebarWidth,
    sidebarWrites,
    upperPct,
    resizeUpperPct,
    commitUpperPct,
    resetUpperPct,
    bumpUpperPctWrites,
    adoptUpperPct,
    upperPctWrites,
    maximizedPanelOf,
    setMaximizedFor,
    adoptMaximized,
    maximizedWrites,
    viewOf,
    rememberedViewFor,
    rememberView,
    adoptViews,
    viewCovers,
    viewWrites,
  } = useViewerPrefs();

  const sidebarRef = useRef<HTMLElement>(null);
  // The two edges of the vertical split, so the divider between the panels can
  // turn a pointer position into a percentage of the region they share.
  const upperRef = useRef<HTMLElement>(null);
  const lowerRef = useRef<HTMLElement>(null);

  const sidebar = useSidebarDrag({
    sidebarRef,
    sidebarWidth,
    resizeSidebar,
    commitSidebarWidth,
    resetSidebarWidth,
    bumpSidebarWrites,
  });
  const split = useUpperPctDrag({
    upperRef,
    lowerRef,
    upperPct,
    resizeUpperPct,
    commitUpperPct,
    resetUpperPct,
    bumpUpperPctWrites,
  });

  return {
    accent,
    next,
    cycle,
    upperPct,
    /** Per-project maximize, bound to the project on screen by the call site —
     *  this hook runs before one has been chosen. */
    maximizedPanelOf,
    setMaximizedFor,
    /** What each project was last showing, bound to the project on screen by
     *  the call site for the same reason the maximize is. */
    viewOf,
    rememberedViewFor,
    rememberView,
    viewCovers,
    /** Everything `RepoShell` needs to render and drive both dividers. */
    shell: {
      sidebarWidth,
      sidebarRef,
      upperRef,
      lowerRef,
      draggingSidebar: sidebar.draggingSidebar,
      onSidebarDragStart: sidebar.onSidebarDragStart,
      onSidebarDragMove: sidebar.onSidebarDragMove,
      onSidebarDragEnd: sidebar.onSidebarDragEnd,
      onSidebarDragCancel: sidebar.onSidebarDragCancel,
      draggingUpper: split.draggingUpper,
      onUpperDragStart: split.onUpperDragStart,
      onUpperDragMove: split.onUpperDragMove,
      onUpperDragEnd: split.onUpperDragEnd,
      onUpperDragCancel: split.onUpperDragCancel,
    },
    /** What keeps the repository poll from adopting a value newer than it. */
    guards: {
      adoptAccent,
      adoptSidebarWidth,
      adoptUpperPct,
      adoptMaximized,
      adoptViews,
      accentWrites,
      sidebarWrites,
      upperPctWrites,
      maximizedWrites,
      viewWrites,
      draggingRef: sidebar.draggingRef,
      upperDraggingRef: split.upperDraggingRef,
    },
  };
}

export type ShellLayout = ReturnType<typeof useShellLayout>["shell"];
