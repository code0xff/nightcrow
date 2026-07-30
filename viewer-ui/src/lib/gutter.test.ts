import { describe, expect, it } from "vitest";
import { digitsFor, linenoDigits } from "./gutter";
import type { DiffHunk, DiffLine } from "../api";

function hunk(lines: DiffLine[]): DiffHunk {
  return { header: "@@", lines };
}

function line(old?: number, added?: number): DiffLine {
  return { kind: " ", spans: [], old_lineno: old, new_lineno: added };
}

describe("digitsFor", () => {
  it("빈_파일도_최소_폭을_받는다", () => {
    expect(digitsFor(0)).toBe(3);
  });

  it("최소_폭보다_짧은_번호는_최소_폭으로_올린다", () => {
    expect(digitsFor(1)).toBe(3);
    expect(digitsFor(99)).toBe(3);
  });

  it("자릿수가_늘면_폭도_늘어난다", () => {
    expect(digitsFor(999)).toBe(3);
    expect(digitsFor(1000)).toBe(4);
    expect(digitsFor(12345)).toBe(5);
  });

  it("십의_거듭제곱_경계에서_한_자리_어긋나지_않는다", () => {
    for (const exp of [3, 4, 5, 6, 7]) {
      const power = 10 ** exp;
      expect(digitsFor(power - 1)).toBe(exp);
      expect(digitsFor(power)).toBe(exp + 1);
    }
  });
});

describe("linenoDigits", () => {
  it("hunk이_없으면_최소_폭을_준다", () => {
    expect(linenoDigits([])).toBe(3);
  });

  it("양쪽_번호_중_가장_큰_값을_기준으로_삼는다", () => {
    expect(linenoDigits([hunk([line(9, 9), line(1200, 8)])])).toBe(4);
    expect(linenoDigits([hunk([line(9, 9)]), hunk([line(70000, 8)])])).toBe(5);
  });

  it("한쪽에만_있는_줄도_폭_계산에_들어간다", () => {
    expect(linenoDigits([hunk([line(undefined, 1000)])])).toBe(4);
    expect(linenoDigits([hunk([line(1000, undefined)])])).toBe(4);
  });

  it("번호가_전혀_없는_diff는_최소_폭으로_떨어진다", () => {
    expect(linenoDigits([hunk([line(), line()])])).toBe(3);
  });
});
