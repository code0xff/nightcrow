// The vertical split between the diff panel and the terminal panel, as the
// diff panel's percentage of the two. Pure math so the drag hook holds only
// pointer bookkeeping.

// Keep these bounds aligned with the server preference limits
// (`src/session/prefs/`), which clamp again on write and on load.
export const MIN_UPPER_PCT = 20;
export const MAX_UPPER_PCT = 85;
export const DEFAULT_UPPER_PCT = 55;

/** Clamp without rounding, for the value the drag shows as it moves.
 *
 *  A non-finite input falls back to the default rather than propagating: this
 *  runs on what a server response and `localStorage` hand over, and `NaN` here
 *  would reach the grid as `NaNfr` and collapse the layout with nothing to
 *  correct it. */
export function clampUpperPctExact(pct: number): number {
  if (!Number.isFinite(pct)) return DEFAULT_UPPER_PCT;
  return Math.min(Math.max(pct, MIN_UPPER_PCT), MAX_UPPER_PCT);
}

/** Round as well as clamp: what gets stored is an integer percent, as the TUI
 *  counterpart it mirrors (`layout.upper_pct`) is. Only the stored value is
 *  rounded — rounding what the drag displays would step the divider a whole
 *  percent at a time, which on a tall window is a visible dozen pixels. */
export function clampUpperPct(pct: number): number {
  return Math.round(clampUpperPctExact(pct));
}

/**
 * Where a pointer at `clientY` puts the divider, given the split region's
 * `top` and `bottom` in the same coordinates.
 *
 * Unlike the sidebar's absolute width, a percentage needs both edges of the
 * region to mean anything — and the region spans two grid tracks with no
 * element of its own, so the caller measures the first track's top and the
 * second's bottom. A region with no height yields the current value rather
 * than a division by zero: a drag cannot start on something not laid out yet.
 *
 * Unrounded, so the divider tracks the pointer; the commit rounds.
 */
export function upperPctAt(
  clientY: number,
  top: number,
  bottom: number,
  current: number,
): number {
  const height = bottom - top;
  if (height <= 0) return clampUpperPctExact(current);
  return clampUpperPctExact(((clientY - top) / height) * 100);
}
