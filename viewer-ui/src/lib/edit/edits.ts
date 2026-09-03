/**
 * Applies edits, expressed in source offsets, in one pass.
 *
 * Marker injection uses source offsets. Applied one at a time front to back, an
 * earlier edit shifts the later offsets and the cut lands in the wrong place, so
 * the edits are collected into one list and applied in descending order.
 */

/** Replaces `[start, end)` with `text`. `start === end` is an insertion. */
export interface Edit {
  start: number;
  end: number;
  text: string;
}

export class EditError extends Error {}

/**
 * Overlapping edits are rejected, not silently overwritten: an overlap means two
 * rules interpreted the same spot differently, so either choice is wrong.
 */
export function applyEdits(source: string, edits: readonly Edit[]): string {
  const ordered = [...edits].sort((a, b) => b.start - a.start || b.end - a.end);

  let out = source;
  // Start of the edit just applied. The next (earlier) edit must not cross it.
  let limit = source.length;
  for (const edit of ordered) {
    if (edit.start < 0 || edit.end > source.length || edit.start > edit.end) {
      throw new EditError(`edit runs past the source: [${edit.start}, ${edit.end})`);
    }
    if (edit.end > limit) {
      throw new EditError(`edits overlap: [${edit.start}, ${edit.end})`);
    }
    out = out.slice(0, edit.start) + edit.text + out.slice(edit.end);
    limit = edit.start;
  }
  return out;
}
