import { useCallback, useState } from "react";
import type { DiffLine } from "../api";

export type DiffLayout = "unified" | "split";

const STORAGE_KEY = "nightcrow.viewer.diffLayout";

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

/** The layout holds at every width — narrow screens stack the two sides. */
export function useDiffLayout() {
  const [layout, setLayout] = useState<DiffLayout>(loadLayout);

  const toggle = useCallback(() => {
    setLayout((current) => {
      const next = current === "split" ? "unified" : "split";
      storeLayout(next);
      return next;
    });
  }, []);

  return { layout, toggle };
}
