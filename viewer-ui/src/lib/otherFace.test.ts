import { describe, expect, it } from "vitest";
import { hasWorkingCopy, otherFace } from "./otherFace";
import type { FileSource, Pane } from "../types";

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

  it("sees the copy a staged deletion left behind", () => {
    // `git rm --cached f` then recreating `f`: the deletion is staged and the
    // file is sitting there. Reading the index column alone would refuse to
    // open something that exists.
    expect(hasWorkingCopy(file("D", "?"))).toBe(true);
    expect(hasWorkingCopy(file("D", "A"))).toBe(true);
  });
});
