// The vertical split between the diff panel and the terminal panel, as the
// diff panel's percentage of the two. Pure math so the drag hook holds only
// pointer bookkeeping.

// Keep these bounds aligned with the server preference limits
// (`web/viewer/prefs`), which clamp again on write and on load.
export const MIN_UPPER_PCT = 20;
export const MAX_UPPER_PCT = 85;
export const DEFAULT_UPPER_PCT = 55;

/** Round as well as clamp: the stored value is an integer percent, and the TUI
 *  counterpart it mirrors (`layout.upper_pct`) is one too. */
export function clampUpperPct(pct: number): number {
  return Math.min(Math.max(Math.round(pct), MIN_UPPER_PCT), MAX_UPPER_PCT);
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
 */
export function upperPctAt(
  clientY: number,
  top: number,
  bottom: number,
  current: number,
): number {
  const height = bottom - top;
  if (height <= 0) return clampUpperPct(current);
  return clampUpperPct(((clientY - top) / height) * 100);
}
