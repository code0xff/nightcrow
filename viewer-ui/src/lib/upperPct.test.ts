import { describe, expect, it } from "vitest";
import {
  DEFAULT_UPPER_PCT,
  MAX_UPPER_PCT,
  MIN_UPPER_PCT,
  clampUpperPct,
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

describe("upperPctAt", () => {
  it("포인터_위치를_구간_안의_비율로_바꾼다", () => {
    // 구간 200..1000 의 절반 지점.
    expect(upperPctAt(600, 200, 1000, DEFAULT_UPPER_PCT)).toBe(50);
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
