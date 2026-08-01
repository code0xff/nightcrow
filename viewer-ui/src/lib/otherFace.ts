import type { ChangedFile, Diff } from "../api";
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
 * Whether a changed file still has a working copy to read whole.
 *
 * A deletion has a diff and nothing behind it: offering the toggle would be a
 * button whose only outcome is an error. The columns are git's short `XY`, and
 * `D` on either side is read as gone.
 *
 * **`D ` is ambiguous and refused anyway.** A staged deletion whose path was
 * recreated — `git rm --cached f`, then `f` again — is `INDEX_DELETED | WT_NEW`
 * to git, and this session's one-row-per-path model deliberately keeps the
 * index side rather than masking it as untracked (`git::diff::snapshot`), so it
 * arrives as `D ` exactly like a file that is really gone. The columns cannot
 * tell them apart, and of the two mistakes — refusing to open a file that is
 * there, and offering to open one that is not — the first is a missing offer
 * and the second is a broken button.
 *
 * The TUI is looser, letting `v` try and reporting the failure. A key that says
 * nothing about itself is not the same as a button that says "open this": one
 * is a question, the other is an offer.
 */
export function hasWorkingCopy(file: ChangedFile): boolean {
  return file.index !== "D" && file.worktree !== "D";
}

/**
 * Whether a diff has text behind it — that is, whether the whole file can be
 * shown at all.
 *
 * Read off the diff rather than asked of the server: `/api/file` refuses a
 * binary outright ("binary or non-utf8 file"), so offering the toggle for a
 * changed PNG is a button whose only outcome is an error.
 *
 * The signal is a line that belongs to a *side*, not a line at all. A binary
 * change is not hunkless — it arrives as one synthetic line, "Binary files
 * differ", carrying neither an old nor a new number, because a binary file has
 * no line numbering (`git::diff::snapshot::binary_diff_hunk`). Counting lines
 * therefore said yes to exactly the case this exists to exclude.
 *
 * A truncated diff counts even with nothing left to show: the ceiling was hit,
 * which means there was text, and that is the case where the whole file is most
 * worth reaching.
 *
 * It costs one case: a mode-only change (`chmod`) has no numbered lines and its
 * file is readable. That is a missing offer, which is the side to be wrong on —
 * the same trade the deleted-path check makes.
 */
export function showsText(diff: Diff): boolean {
  if (diff.truncated) return true;
  return diff.hunks.some((hunk) =>
    hunk.lines.some(
      (line) => line.old_lineno !== undefined || line.new_lineno !== undefined,
    ),
  );
}
