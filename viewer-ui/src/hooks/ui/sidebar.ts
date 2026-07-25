// localStorage provides first paint while the server shares this preference.

import { useCallback, useState } from "react";
import { api } from "../../api";

const STORAGE_KEY = "nightcrow.sidebarWidth";

// Keep these bounds aligned with the server preference limits.
export const MIN_SIDEBAR_WIDTH = 280;
export const MAX_SIDEBAR_WIDTH = 720;
export const DEFAULT_SIDEBAR_WIDTH = 460;
/** Display cap; stored preferences remain absolute widths. */
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

export function useSidebarWidth() {
  const [width, setWidth] = useState(loadWidth);

  const resize = useCallback((px: number) => {
    const clamped = clampSidebarDragWidth(px);
    setWidth(clamped);
    storeWidth(clamped);
  }, []);

  const commit = useCallback((px: number) => {
    const clamped = clampSidebarDragWidth(px);
    setWidth(clamped);
    storeWidth(clamped);
    void api.setSidebarWidth(clamped).catch(() => {
    });
  }, []);

  const reset = useCallback(() => {
    const clamped = clampSidebarWidth(DEFAULT_SIDEBAR_WIDTH);
    setWidth(clamped);
    storeWidth(clamped);
    void api.setSidebarWidth(clamped).catch(() => {
    });
  }, []);

  /** Apply remote values without echoing them back. */
  const adopt = useCallback((remote: number) => {
    setWidth((current) => {
      const clamped = clampSidebarWidth(remote);
      if (clamped === current) return current;
      storeWidth(clamped);
      return clamped;
    });
  }, []);

  return { width, resize, commit, reset, adopt };
}
