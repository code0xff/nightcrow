import { useCallback, useState } from "react";
import type { DiffLine } from "../api";

export type DiffLayout = "unified" | "split";

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

/**
 * Unified is the default at every width; split is something you reach for on a
 * diff that needs it, so the choice lasts the session and is not stored — the
 * same lifetime the TUI gives it. Narrow screens stack the two sides rather
 * than falling back to unified.
 */
export function useDiffLayout() {
  const [layout, setLayout] = useState<DiffLayout>("unified");

  const toggle = useCallback(() => {
    setLayout((current) => (current === "split" ? "unified" : "split"));
  }, []);

  return { layout, toggle };
}
