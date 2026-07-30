import { describe, expect, it } from "vitest";
import { digitsFor, sideGutterWidth } from "./gutter";

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

describe("sideGutterWidth", () => {
  it("숫자_칼럼_양옆에_한_칸씩_더한_폭을_준다", () => {
    expect(sideGutterWidth(3)).toBe("5ch");
    expect(sideGutterWidth(5)).toBe("7ch");
  });
});
