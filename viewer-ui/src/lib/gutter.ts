/**
 * Line-number gutter arithmetic, mirroring the TUI's `ui/diff_viewer/gutter.rs`
 * so the same file reads the same width in both frontends. The geometry the
 * digits feed lives in `components/LineNos.tsx`.
 */
import type { DiffHunk } from "../api";

/** Minimum digits reserved for one line-number column. Keeps the gutter — and
 *  with it the body's left edge — from twitching between a 99-line file and a
 *  100-line one. */
const MIN_LINENO_DIGITS = 3;

/** Digits needed to print `maxLineno`, floored at `MIN_LINENO_DIGITS`.
 *
 *  Counts the decimal string rather than taking a `log10`, which is off by one
 *  for exact powers of ten once floating point rounds the wrong way. */
export function digitsFor(maxLineno: number): number {
  const digits = maxLineno < 1 ? 1 : String(Math.floor(maxLineno)).length;
  return Math.max(digits, MIN_LINENO_DIGITS);
}

/** Digits for a whole diff: the widest line number on either side of any hunk.
 *
 *  Derived from the loaded diff rather than the rows currently on screen, so
 *  scrolling cannot change the gutter width and shift the body sideways. */
export function linenoDigits(hunks: DiffHunk[]): number {
  let max = 0;
  for (const hunk of hunks) {
    for (const line of hunk.lines) {
      max = Math.max(max, line.old_lineno ?? 0, line.new_lineno ?? 0);
    }
  }
  return digitsFor(max);
}
