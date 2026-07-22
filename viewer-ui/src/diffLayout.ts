import { useCallback, useEffect, useState } from "react";
import type { DiffLine } from "./api";

/**
 * Diff pane layout preference, persisted for the whole viewer (not per repo,
 * for the same reason as the accent — repo ids only live for the process,
 * `viewer/catalog.rs`). Mirrors the TUI's `DiffPaneView::{Diff, Split}` toggle
 * (`diff_load.rs` `toggle_diff_split_view`).
 */
export type DiffLayout = "unified" | "split";

const STORAGE_KEY = "nightcrow.viewer.diffLayout";

/**
 * Below this viewport width two code columns cannot both stay legible, so a
 * `split` preference renders unified — the web analogue of the TUI's
 * `MIN_SPLIT_WIDTH` fallback (`diff_viewer.rs`). Measured against the viewport
 * rather than the pane: the screens this guards (phones) are narrow throughout,
 * so the extra precision of a `ResizeObserver` on the pane earns nothing here.
 */
const MIN_SPLIT_WIDTH_PX = 768;

/** One side-by-side row: either cell is `null` when that side has no line. */
export interface SplitRow {
  left: DiffLine | null;
  right: DiffLine | null;
}

/**
 * Pair a hunk's lines into side-by-side rows, porting the TUI's `split_rows` /
 * `flush_split_blocks` (`diff_pane.rs`). Consecutive removed/added lines are
 * paired index-by-index with the shorter run padded by `null`; context lines
 * flush the pending block and mirror onto both sides. Kinds are the DTO codes
 * (`dto.rs` `line_code`): `-` removed, `+` added, anything else context.
 */
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
  // localStorage throws (not returns null) when storage is blocked — Safari
  // private mode, site data disabled. Falling back to unified keeps the pane
  // usable, matching `theme.ts`'s handling of the same failure.
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
    // Preference is lost on reload; the session still renders correctly.
  }
}

function matchWide(): boolean {
  try {
    return window.matchMedia(`(min-width: ${MIN_SPLIT_WIDTH_PX}px)`).matches;
  } catch {
    // No matchMedia (very old/embedded browsers): assume wide so split works.
    return true;
  }
}

/** Tracks whether the viewport is wide enough for split view. */
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

/**
 * Current diff layout preference plus a toggle, mirroring `useAccent`. `layout`
 * is the stored preference (drives the toggle's pressed state); `effective`
 * collapses to unified when the viewport is too narrow, so widening the window
 * restores the user's split preference without a second click.
 */
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
