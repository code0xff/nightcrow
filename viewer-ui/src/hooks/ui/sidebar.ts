// Device-local, like the pane view mode: a width in pixels is a fact about the
// screen in front of you. Shared through `viewer.json` it was one value for
// every screen — 720px is a third of a wide monitor and half of a laptop — and
// the page had to cap it against the viewport on every render to stay usable.
// That cap was the symptom; keeping the value where the screen is removes the
// cause. The panel split (`upperPct.ts`) stays shared because a percentage
// means the same thing at any width.

import { useCallback, useState } from "react";

const STORAGE_KEY = "nightcrow.sidebarWidth";

export const MIN_SIDEBAR_WIDTH = 280;
export const MAX_SIDEBAR_WIDTH = 720;
export const DEFAULT_SIDEBAR_WIDTH = 460;
/** Display cap, for a window that has since been made narrower than the width
 *  it stored; the stored value remains an absolute width. */
export const MAX_SIDEBAR_VIEWPORT_FRACTION = 0.5;

export function clampSidebarWidth(px: number): number {
  // Keep stored widths integral.
  return Math.min(Math.max(Math.round(px), MIN_SIDEBAR_WIDTH), MAX_SIDEBAR_WIDTH);
}

export function clampSidebarDragWidth(px: number): number {
  let ceiling = MAX_SIDEBAR_WIDTH;
  try {
    ceiling = Math.min(
      ceiling,
      Math.round(window.innerWidth * MAX_SIDEBAR_VIEWPORT_FRACTION),
    );
  } catch {
  }
  return Math.min(
    Math.max(Math.round(px), MIN_SIDEBAR_WIDTH),
    Math.max(ceiling, MIN_SIDEBAR_WIDTH),
  );
}

function loadWidth(): number {
  try {
    const raw = Number(localStorage.getItem(STORAGE_KEY));
    return Number.isFinite(raw) && raw > 0
      ? clampSidebarWidth(raw)
      : DEFAULT_SIDEBAR_WIDTH;
  } catch {
    return DEFAULT_SIDEBAR_WIDTH;
  }
}

function storeWidth(px: number) {
  try {
    localStorage.setItem(STORAGE_KEY, String(px));
  } catch {
  }
}

/**
 * The file sidebar's width. `resize` is the drag in progress and `commit` its
 * release; both store, so a drag interrupted by a reload keeps what it reached.
 */
export function useSidebarWidth() {
  const [width, setWidth] = useState(loadWidth);

  const resize = useCallback((px: number) => {
    const clamped = clampSidebarDragWidth(px);
    setWidth(clamped);
    storeWidth(clamped);
  }, []);

  const reset = useCallback(() => {
    const clamped = clampSidebarWidth(DEFAULT_SIDEBAR_WIDTH);
    setWidth(clamped);
    storeWidth(clamped);
  }, []);

  return { width, resize, commit: resize, reset };
}
