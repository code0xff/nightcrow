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
 *
 * Several files is more than one *distinct* `file_path`, not the presence of
 * one: every hunk from a commit carries it, including the single-file diff the
 * log drill-down asks for. Reading it as a marker of a whole-commit diff refused
 * an anchor for exactly the case this feature exists to serve.
 */
export function anchorLine(diff: Diff): number | null {
  const files = new Set(diff.hunks.map((hunk) => hunk.file_path ?? diff.path));
  if (files.size > 1) return null;
  for (const hunk of diff.hunks) {
    for (const line of hunk.lines) {
      if (line.new_lineno !== undefined) return line.new_lineno;
    }
  }
  return null;
}

/**
 * The 1-based line to put at the top so the anchor sits just below it.
 *
 * Two lines of context, the same landing the TUI gives it.
 */
export function anchorOffset(line: number): number {
  return Math.max(0, line - 1 - 2);
}

/**
 * The line to actually scroll to, given how many the file turned out to have.
 *
 * A file can be shorter than the change asked about — the server truncates at a
 * size ceiling, and the anchor is read from a diff that knew nothing of that.
 * Scrolling to the end of what there is beats ignoring the request and opening
 * at the top, which reads as the switch having done nothing.
 */
export function anchorWithin(anchor: number, lineCount: number): number | null {
  if (lineCount <= 0) return null;
  return Math.min(anchor, lineCount);
}
