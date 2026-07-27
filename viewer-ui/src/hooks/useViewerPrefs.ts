import { useCallback, useRef } from "react";
import { useAccent } from "./ui/theme";
import { useSidebarWidth } from "./ui/sidebar";

/**
 * The preferences this page owns locally, and the bookkeeping that keeps the
 * repository poll from undoing them.
 *
 * Accent and sidebar width live on the server so every device agrees, and the
 * poll adopts what it reads. But a value the user just changed here is newer
 * than anything a request in flight can carry, so every local write bumps a
 * counter; the poll compares the counter it started with and skips adopting
 * when it moved. Wrapping each setter with its bump is why they are exposed
 * from one place rather than assembled at the call site — a write that forgets
 * to count silently reverts a moment later.
 */
export function useViewerPrefs() {
  const { accent, next, cycle: cycleAccent, adopt: adoptAccent } = useAccent();
  const {
    width: sidebarWidth,
    resize: resizeSidebar,
    commit: commitSidebar,
    reset: resetSidebar,
    adopt: adoptSidebarWidth,
  } = useSidebarWidth();
  const accentWrites = useRef(0);
  const sidebarWrites = useRef(0);

  const cycle = useCallback(() => {
    accentWrites.current += 1;
    cycleAccent();
  }, [cycleAccent]);
  const commitSidebarWidth = useCallback(
    (px: number) => {
      sidebarWrites.current += 1;
      commitSidebar(px);
    },
    [commitSidebar],
  );
  const resetSidebarWidth = useCallback(() => {
    sidebarWrites.current += 1;
    resetSidebar();
  }, [resetSidebar]);
  // For a write this hook does not perform itself — the drag's own commit.
  const bumpSidebarWrites = useCallback(() => {
    sidebarWrites.current += 1;
  }, []);

  return {
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
  };
}
