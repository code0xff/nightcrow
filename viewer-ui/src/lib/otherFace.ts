import type { ChangedFile } from "../api";
import type { FileSource, Pane } from "../types";

/**
 * What a pane's other face is, or `null` when it has none.
 *
 * The one decision behind the whole-file toggle, kept apart from the fetching so
 * it can be checked: which of the two to ask for, and about what. `null` is also
 * what decides whether the control appears at all — a pane with no second face
 * must not offer one, and expressing that as the absence of an answer rather
 * than a condition beside the button keeps the two from drifting.
 */
export function otherFace(
  pane: Pane,
): { want: "file" | "diff"; source: FileSource } | null {
  if (pane.kind === "empty" || !pane.source) return null;
  return { want: pane.kind === "diff" ? "file" : "diff", source: pane.source };
}

/**
 * Whether a changed file still has a working copy to show whole.
 *
 * A deletion has a diff and nothing to read: offering the toggle for it would
 * be a button whose only outcome is an error. The status columns are git's
 * short `XY` — `D` on either side is the file being gone from the tree, staged
 * or not.
 *
 * The TUI is looser here, letting `v` try and reporting the failure. A key that
 * says nothing is not the same as a button that says "open this": one is a
 * question, the other is an offer.
 */
export function hasWorkingCopy(file: ChangedFile): boolean {
  return file.index !== "D" && file.worktree !== "D";
}
