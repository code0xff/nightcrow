import { describe, expect, it } from "vitest";
import { digitsFor, linenoDigits } from "./gutter";
import type { DiffHunk, DiffLine } from "../../api";

function hunk(lines: DiffLine[]): DiffHunk {
  return { header: "@@", lines };
}

function line(old?: number, added?: number): DiffLine {
  return { kind: " ", spans: [], old_lineno: old, new_lineno: added };
}

describe("digitsFor", () => {
  it("an empty file still gets the minimum width", () => {
    expect(digitsFor(0)).toBe(3);
  });

  it("numbers shorter than the minimum width are padded to the minimum", () => {
    expect(digitsFor(1)).toBe(3);
    expect(digitsFor(99)).toBe(3);
  });

  it("width increases as the number of digits grows", () => {
    expect(digitsFor(999)).toBe(3);
    expect(digitsFor(1000)).toBe(4);
    expect(digitsFor(12345)).toBe(5);
  });

  it("does not miscount a digit at a power-of-ten boundary", () => {
    for (const exp of [3, 4, 5, 6, 7]) {
      const power = 10 ** exp;
      expect(digitsFor(power - 1)).toBe(exp);
      expect(digitsFor(power)).toBe(exp + 1);
    }
  });
});

describe("linenoDigits", () => {
  it("uses the minimum width when there are no hunks", () => {
    expect(linenoDigits([])).toBe(3);
  });

  it("uses the larger of the two line numbers", () => {
    expect(linenoDigits([hunk([line(9, 9), line(1200, 8)])])).toBe(4);
    expect(linenoDigits([hunk([line(9, 9)]), hunk([line(70000, 8)])])).toBe(5);
  });

  it("includes lines present on only one side in the width calculation", () => {
    expect(linenoDigits([hunk([line(undefined, 1000)])])).toBe(4);
    expect(linenoDigits([hunk([line(1000, undefined)])])).toBe(4);
  });

  it("falls back to the minimum width for a diff with no line numbers", () => {
    expect(linenoDigits([hunk([line(), line()])])).toBe(3);
  });
});
