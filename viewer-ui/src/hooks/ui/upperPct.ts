// localStorage provides first paint while the server shares this preference.

import { useCallback, useState } from "react";
import { api } from "../../api";
import {
  DEFAULT_UPPER_PCT,
  clampUpperPct,
  clampUpperPctExact,
} from "../../lib/upperPct";

const STORAGE_KEY = "nightcrow.upperPct";

function loadPct(): number {
  try {
    const raw = Number(localStorage.getItem(STORAGE_KEY));
    return Number.isFinite(raw) && raw > 0
      ? clampUpperPct(raw)
      : DEFAULT_UPPER_PCT;
  } catch {
    return DEFAULT_UPPER_PCT;
  }
}

function storePct(pct: number) {
  try {
    localStorage.setItem(STORAGE_KEY, String(pct));
  } catch {
  }
}

/**
 * The share of the vertical split the diff panel gets.
 *
 * Shaped like [`useSidebarWidth`], with one bound instead of two: a percentage
 * is already relative to whatever screen reads it, so there is no viewport cap
 * to apply on top of the stored value the way an absolute width needs one.
 */
export function useUpperPct() {
  const [pct, setPct] = useState(loadPct);

  // Exact while dragging so the divider stays under the pointer; what lands in
  // storage is the rounded value the release commits.
  const resize = useCallback((next: number) => {
    setPct(clampUpperPctExact(next));
  }, []);

  const commit = useCallback((next: number) => {
    const clamped = clampUpperPct(next);
    setPct(clamped);
    storePct(clamped);
    void api.setUpperPct(clamped).catch(() => {
    });
  }, []);

  const reset = useCallback(() => {
    setPct(DEFAULT_UPPER_PCT);
    storePct(DEFAULT_UPPER_PCT);
    void api.setUpperPct(DEFAULT_UPPER_PCT).catch(() => {
    });
  }, []);

  /** Apply remote values without echoing them back. */
  const adopt = useCallback((remote: number) => {
    setPct((current) => {
      const clamped = clampUpperPct(remote);
      if (clamped === current) return current;
      storePct(clamped);
      return clamped;
    });
  }, []);

  return { pct, resize, commit, reset, adopt };
}
