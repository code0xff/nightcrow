import { describe, expect, it } from "vitest";
import { anchorLine, anchorOffset } from "./diffAnchor";
import type { Diff, DiffLine } from "../api";

const line = (new_lineno?: number): DiffLine => ({
  kind: new_lineno === undefined ? "-" : " ",
  spans: [],
  new_lineno,
});

const diff = (hunks: Diff["hunks"]): Diff => ({
  path: "a.rs",
  hunks,
  truncated: false,
});

describe("anchorLine", () => {
  it("takes the first line the change has on the new side", () => {
    expect(
      anchorLine(diff([{ header: "@@", lines: [line(41), line(42)] }])),
    ).toBe(41);
  });

  it("skips lines that exist only on the old side", () => {
    // A hunk opening with removals: the first line to land on is the first one
    // the file still has.
    expect(
      anchorLine(diff([{ header: "@@", lines: [line(), line(), line(7)] }])),
    ).toBe(7);
  });

  it("has no answer for a wholly deleted file", () => {
    expect(anchorLine(diff([{ header: "@@", lines: [line(), line()] }]))).toBeNull();
  });

  it("has no answer for a diff spanning several files", () => {
    // The numbers belong to whichever file each hunk came from, so reading them
    // as one sequence would land somewhere meaningless.
    expect(
      anchorLine(
        diff([
          { header: "@@", file_path: "a.rs", lines: [line(3)] },
          { header: "@@", file_path: "b.rs", lines: [line(90)] },
        ]),
      ),
    ).toBeNull();
  });

  it("has no answer for an empty diff", () => {
    expect(anchorLine(diff([]))).toBeNull();
  });
});

describe("anchorOffset", () => {
  it("leaves two lines above the change", () => {
    expect(anchorOffset(41)).toBe(38);
  });

  it("does not scroll past the top for a change near it", () => {
    expect(anchorOffset(1)).toBe(0);
    expect(anchorOffset(2)).toBe(0);
    expect(anchorOffset(3)).toBe(0);
    expect(anchorOffset(4)).toBe(1);
  });
});
