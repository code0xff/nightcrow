// Device-local, unlike the accent or the panel split: what a phone should do
// with four panes is not what the desktop beside it should do, so this one is
// not shared with the server.

import { useCallback, useEffect, useState } from "react";
import {
  GRID_MIN_VIEWPORT_PX,
  defaultPaneViewMode,
  parsePaneViewMode,
  type PaneViewMode,
} from "../../lib/paneViewMode";

const STORAGE_KEY = "nightcrow.paneViewMode";

function load(): PaneViewMode | null {
  try {
    return parsePaneViewMode(localStorage.getItem(STORAGE_KEY));
  } catch {
    return null;
  }
}

function store(mode: PaneViewMode) {
  try {
    localStorage.setItem(STORAGE_KEY, mode);
  } catch {
  }
}

function viewportWidth(): number {
  return typeof window === "undefined" ? GRID_MIN_VIEWPORT_PX : window.innerWidth;
}

/**
 * Whether the panel draws its panes as a grid or as tabs.
 *
 * The width decides until someone says otherwise, and then their choice sticks
 * — including across the rotation that would otherwise flip it back. Kept in one
 * place because the two halves have to agree: the toggle writes the override,
 * and the width is only consulted while there is none.
 */
export function usePaneViewMode() {
  const [override, setOverride] = useState(load);
  const [width, setWidth] = useState(viewportWidth);

  useEffect(() => {
    if (typeof window === "undefined") return;
    const apply = () => setWidth(window.innerWidth);
    apply();
    window.addEventListener("resize", apply);
    return () => window.removeEventListener("resize", apply);
  }, []);

  const mode = override ?? defaultPaneViewMode(width);

  const toggle = useCallback(() => {
    const next: PaneViewMode = mode === "tabs" ? "grid" : "tabs";
    store(next);
    setOverride(next);
  }, [mode]);

  return { mode, toggle };
}
