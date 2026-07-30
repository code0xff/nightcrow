/**
 * Line-number gutter geometry, mirroring the TUI's `ui/diff_viewer/gutter.rs`
 * so the same file reads the same width in both frontends.
 */

/** Minimum digits reserved for one line-number column. Keeps the gutter — and
 *  with it the body's left edge — from twitching between a 99-line file and a
 *  100-line one. */
const MIN_LINENO_DIGITS = 3;

/** One padding space on each side of a number column: it lifts the digits off
 *  the pane edge and off the code that follows. */
const LINENO_PAD = 2;

/** Digits needed to print `maxLineno`, floored at `MIN_LINENO_DIGITS`.
 *
 *  Counts the decimal string rather than taking a `log10`, which is off by one
 *  for exact powers of ten once floating point rounds the wrong way. */
export function digitsFor(maxLineno: number): number {
  const digits = maxLineno < 1 ? 1 : String(Math.floor(maxLineno)).length;
  return Math.max(digits, MIN_LINENO_DIGITS);
}

/** CSS width of a one-column gutter, in `ch` so it tracks the mono font. */
export function sideGutterWidth(digits: number): string {
  return `${digits + LINENO_PAD}ch`;
}
