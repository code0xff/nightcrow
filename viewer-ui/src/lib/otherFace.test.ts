import { describe, expect, it } from "vitest";
import { hasWorkingCopy, otherFace, showsText } from "./otherFace";
import type { DiffLine } from "../api";
import type { FileSource, Pane } from "../types";

const line = (new_lineno?: number): DiffLine => ({
  kind: " ",
  spans: [],
  new_lineno,
});

const diff = (source?: FileSource): Pane => ({
  kind: "diff",
  value: { path: "a.rs", hunks: [], truncated: false },
  source,
});

describe("otherFace", () => {
  it("offers the whole file from a working-tree diff", () => {
    expect(otherFace(diff({ kind: "workdir", path: "a.rs" }))).toEqual({
      want: "file",
      source: { kind: "workdir", path: "a.rs" },
    });
  });

  it("offers the whole file from a commit's diff, at that commit", () => {
    expect(otherFace(diff({ kind: "commit", oid: "abc", path: "a.rs" }))).toEqual({
      want: "file",
      source: { kind: "commit", oid: "abc", path: "a.rs" },
    });
  });

  it("offers the diff back from the file", () => {
    const pane: Pane = {
      kind: "file",
      value: { path: "a.rs", lines: [], truncated: false },
      source: { kind: "commit", oid: "abc", path: "a.rs" },
    };
    expect(otherFace(pane)?.want).toBe("diff");
  });

  it("has nothing to offer for a diff spanning a whole commit", () => {
    // No source: several files, so "which one" has no answer.
    expect(otherFace(diff(undefined))).toBeNull();
  });

  it("has nothing to offer for a file opened from the tree", () => {
    const pane: Pane = {
      kind: "file",
      value: { path: "a.rs", lines: [], truncated: false },
    };
    expect(otherFace(pane)).toBeNull();
  });

  it("has nothing to offer for an empty pane", () => {
    expect(otherFace({ kind: "empty" })).toBeNull();
  });
});

describe("hasWorkingCopy", () => {
  const file = (index: string, worktree: string) => ({
    path: "a.rs",
    index,
    worktree,
  });

  it("sees a copy for an ordinary modification", () => {
    expect(hasWorkingCopy(file(" ", "M"))).toBe(true);
    expect(hasWorkingCopy(file("M", " "))).toBe(true);
    expect(hasWorkingCopy(file("A", " "))).toBe(true);
    expect(hasWorkingCopy(file("?", "?"))).toBe(true);
  });

  it("sees none for a deletion, staged or not", () => {
    expect(hasWorkingCopy(file("D", " "))).toBe(false);
    expect(hasWorkingCopy(file(" ", "D"))).toBe(false);
    expect(hasWorkingCopy(file("D", "D"))).toBe(false);
  });

  it("refuses the ambiguous staged deletion rather than guess", () => {
    // `git rm --cached f` then recreating `f` arrives as `D ` — the same row a
    // real deletion produces, because the one-row-per-path model keeps the
    // index side rather than masking it as untracked. The file is there and
    // this will not offer to open it; of the two mistakes available, a missing
    // offer beats a button that only errors.
    expect(hasWorkingCopy(file("D", " "))).toBe(false);
  });
});

describe("showsText", () => {
  const hunk = (lines: DiffLine[]) => ({ header: "@@", lines });

  it("sees text where a line belongs to a side", () => {
    expect(
      showsText({ path: "a.rs", hunks: [hunk([line(3)])], truncated: false }),
    ).toBe(true);
  });

  it("sees none in a binary change, which is one unnumbered line", () => {
    // Not hunkless: git's binary delta arrives as a synthetic "Binary files
    // differ" line with neither an old nor a new number. Counting lines said
    // yes to exactly the case this exists to exclude.
    const synthetic: DiffLine = { kind: " ", spans: [] };
    expect(
      showsText({
        path: "logo.png",
        hunks: [hunk([synthetic])],
        truncated: false,
      }),
    ).toBe(false);
  });

  it("sees none for a diff with no hunks at all", () => {
    expect(showsText({ path: "a.rs", hunks: [], truncated: false })).toBe(false);
  });

  it("sees text in a diff truncated to nothing", () => {
    // The ceiling was hit, so there was text — and that is the case where
    // reaching the whole file matters most.
    expect(showsText({ path: "big.rs", hunks: [], truncated: true })).toBe(true);
  });
});
