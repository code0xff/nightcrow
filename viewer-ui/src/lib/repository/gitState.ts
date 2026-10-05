// How the repository's git state reads on screen: a stopped merge or rebase,
// and the refs on a commit. Worded and ordered the way the TUI's notice row
// and commit list do it (`src/git/diff/operation.rs`, `src/ui/commit_list/
// row.rs`), so the two faces of a session say the same thing.

import type { CommitRef, Status } from "../../api";

const OPERATION_LABEL: Record<NonNullable<Status["operation"]>["kind"], string> = {
  merge: "MERGING",
  rebase: "REBASING",
  "cherry-pick": "CHERRY-PICKING",
  revert: "REVERTING",
  bisect: "BISECTING",
  am: "APPLYING",
};

/**
 * `REBASING 2/5 · 3 conflicts`, or null for a repository at rest.
 *
 * Conflicts alone still say something: a stash pop or checkout that conflicts
 * leaves no operation behind, and the unmerged files are what needs acting on.
 */
export function operationText(
  status: Pick<Status, "operation" | "conflicts"> | null | undefined,
): string | null {
  const op = status?.operation;
  const opText = op
    ? op.step !== undefined && op.total !== undefined
      ? `${OPERATION_LABEL[op.kind]} ${op.step}/${op.total}`
      : OPERATION_LABEL[op.kind]
    : null;
  const count = status?.conflicts ?? 0;
  const conflictText =
    count === 0 ? null : count === 1 ? "1 conflict" : `${count} conflicts`;
  if (opText && conflictText) return `${opText} · ${conflictText}`;
  return opText ?? conflictText;
}

/** A ref chip's text: the branch HEAD is on reads `HEAD → dev`. */
export function refText(ref: CommitRef): string {
  return ref.kind === "head" && ref.name !== "HEAD" ? `HEAD → ${ref.name}` : ref.name;
}

/**
 * Chip colour by kind, inside the viewer's palette rather than git's: the
 * accent is where you are, green is a branch you could commit on, and tags and
 * remote branches recede to grey. The order already ranks them.
 */
export function refClass(kind: CommitRef["kind"]): string {
  switch (kind) {
    case "head":
      return "border-accent text-accent font-medium";
    case "local":
      return "border-added/60 text-added";
    case "tag":
      return "border-ink-600 text-ink-200";
    case "remote":
      return "border-ink-700 text-ink-400";
  }
}
