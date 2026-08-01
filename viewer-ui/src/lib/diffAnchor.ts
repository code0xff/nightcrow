import type { Diff } from "../api";

/**
 * The line of the whole file to open at, given the diff being left.
 *
 * Switching to the file is not "show me this file" but "show me around this
 * change" — landing at the top of a two-thousand-line file, after asking about
 * a hunk near the end, is the same as not having gone anywhere.
 *
 * The first hunk's first new-side line. The TUI picks the hunk the cursor has
 * scrolled to (`anchor_for_current_diff`); the browser's diff is a scroll
 * container with no cursor in it, and reading which hunk is on screen would
 * mean measuring the DOM. The first hunk is where a diff opens, so for anyone
 * who has not scrolled it is the same answer.
 *
 * `null` when nothing in the diff has a new side — a wholly deleted file — and
 * for a diff spanning several files, where the line numbers belong to whichever
 * file the hunk came from and cannot be read as one sequence.
 */
export function anchorLine(diff: Diff): number | null {
  const spansSeveralFiles = diff.hunks.some((hunk) => hunk.file_path);
  if (spansSeveralFiles) return null;
  for (const hunk of diff.hunks) {
    for (const line of hunk.lines) {
      if (line.new_lineno !== undefined) return line.new_lineno;
    }
  }
  return null;
}

/**
 * How far to scroll so the anchor is visible with a little above it.
 *
 * Two lines of context, and 1-based line numbers into a 0-based offset — the
 * same landing the TUI gives it. Clamped by the caller against the file's
 * length, which this does not know.
 */
export function anchorOffset(line: number): number {
  return Math.max(0, line - 1 - 2);
}
