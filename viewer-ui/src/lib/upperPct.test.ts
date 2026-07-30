import { describe, expect, it } from "vitest";
import {
  DEFAULT_UPPER_PCT,
  MAX_UPPER_PCT,
  MIN_UPPER_PCT,
  clampUpperPct,
  clampUpperPctExact,
  upperPctAt,
} from "./upperPct";

describe("clampUpperPct", () => {
  it("범위_안의_값은_정수로_반올림해서_그대로_돌려준다", () => {
    expect(clampUpperPct(55)).toBe(55);
    expect(clampUpperPct(54.6)).toBe(55);
  });

  it("범위를_벗어나면_가까운_경계로_접는다", () => {
    expect(clampUpperPct(0)).toBe(MIN_UPPER_PCT);
    expect(clampUpperPct(-40)).toBe(MIN_UPPER_PCT);
    expect(clampUpperPct(100)).toBe(MAX_UPPER_PCT);
  });
});

describe("clampUpperPctExact", () => {
  it("반올림하지_않고_범위만_적용한다", () => {
    expect(clampUpperPctExact(54.6)).toBe(54.6);
    expect(clampUpperPctExact(100)).toBe(MAX_UPPER_PCT);
  });

  it("유한하지_않은_값은_기본값으로_바꾼다", () => {
    // 서버 응답이나 localStorage에서 온 값이라, NaN이 그대로 흐르면
    // grid track이 `NaNfr`이 되어 레이아웃이 무너진다.
    for (const bad of [NaN, Infinity, -Infinity]) {
      expect(clampUpperPctExact(bad)).toBe(DEFAULT_UPPER_PCT);
    }
    expect(clampUpperPct(NaN)).toBe(DEFAULT_UPPER_PCT);
  });
});

describe("upperPctAt", () => {
  it("포인터_위치를_구간_안의_비율로_바꾼다", () => {
    // 구간 200..1000 의 절반 지점.
    expect(upperPctAt(600, 200, 1000, DEFAULT_UPPER_PCT)).toBe(50);
  });

  it("반올림하지_않아_포인터를_그대로_따라간다", () => {
    // 1000px 구간에서 1px 움직이면 0.1% — 반올림하면 divider가 포인터를
    // 놓치고 한 퍼센트씩 계단으로 움직인다.
    expect(upperPctAt(501, 0, 1000, DEFAULT_UPPER_PCT)).toBeCloseTo(50.1);
  });

  it("구간_밖으로_끌어도_경계를_넘지_않는다", () => {
    expect(upperPctAt(-500, 0, 800, DEFAULT_UPPER_PCT)).toBe(MIN_UPPER_PCT);
    expect(upperPctAt(5000, 0, 800, DEFAULT_UPPER_PCT)).toBe(MAX_UPPER_PCT);
  });

  it("구간_높이가_0이면_현재_값을_유지한다", () => {
    // 아직 레이아웃되지 않은 영역에서 0으로 나누지 않는다.
    expect(upperPctAt(400, 500, 500, 40)).toBe(40);
  });

  it("높이가_0일_때_현재_값도_범위로_접는다", () => {
    expect(upperPctAt(400, 500, 500, 0)).toBe(MIN_UPPER_PCT);
  });
});
