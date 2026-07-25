import { useCallback, useEffect, useState } from "react";
import type { DiffLine } from "./api";

export type DiffLayout = "unified" | "split";

const STORAGE_KEY = "nightcrow.viewer.diffLayout";

/** Below this width, split falls back to unified. */
const MIN_SPLIT_WIDTH_PX = 768;

export interface SplitRow {
  left: DiffLine | null;
  right: DiffLine | null;
}

/** Pair removed/added runs by index and mirror context on both sides. */
export function splitHunkRows(lines: DiffLine[]): SplitRow[] {
  const rows: SplitRow[] = [];
  let removed: DiffLine[] = [];
  let added: DiffLine[] = [];

  const flush = () => {
    const pairs = Math.max(removed.length, added.length);
    for (let i = 0; i < pairs; i++) {
      rows.push({ left: removed[i] ?? null, right: added[i] ?? null });
    }
    removed = [];
    added = [];
  };

  for (const line of lines) {
    if (line.kind === "-") {
      removed.push(line);
    } else if (line.kind === "+") {
      added.push(line);
    } else {
      flush();
      rows.push({ left: line, right: line });
    }
  }
  flush();
  return rows;
}

function loadLayout(): DiffLayout {
  try {
    return localStorage.getItem(STORAGE_KEY) === "split" ? "split" : "unified";
  } catch {
    return "unified";
  }
}

function storeLayout(layout: DiffLayout) {
  try {
    localStorage.setItem(STORAGE_KEY, layout);
  } catch {
  }
}

function matchWide(): boolean {
  try {
    return window.matchMedia(`(min-width: ${MIN_SPLIT_WIDTH_PX}px)`).matches;
  } catch {
    return true;
  }
}

function useIsWide(): boolean {
  const [wide, setWide] = useState(matchWide);

  useEffect(() => {
    let mql: MediaQueryList;
    try {
      mql = window.matchMedia(`(min-width: ${MIN_SPLIT_WIDTH_PX}px)`);
    } catch {
      return;
    }
    const onChange = (event: MediaQueryListEvent) => setWide(event.matches);
    mql.addEventListener("change", onChange);
    return () => mql.removeEventListener("change", onChange);
  }, []);

  return wide;
}

/** Preserve the preference while exposing a viewport-compatible layout. */
export function useDiffLayout() {
  const [layout, setLayout] = useState<DiffLayout>(loadLayout);
  const wide = useIsWide();

  const toggle = useCallback(() => {
    setLayout((current) => {
      const next = current === "split" ? "unified" : "split";
      storeLayout(next);
      return next;
    });
  }, []);

  const effective: DiffLayout = layout === "split" && wide ? "split" : "unified";

  return { layout, effective, toggle };
}
