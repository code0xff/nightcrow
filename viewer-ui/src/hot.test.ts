import { describe, expect, it } from "vitest";
import { anyHot, classifyHot, clockOffset } from "./hot";

const WINDOW = 10_000;
const NOW = 100_000;

describe("classifyHot", () => {
  it("나이에_따라_fresh_warm_cool로_나눈다", () => {
    expect(classifyHot(NOW - 1_000, NOW, WINDOW)).toBe("fresh");
    expect(classifyHot(NOW - 6_000, NOW, WINDOW)).toBe("warm");
    expect(classifyHot(NOW - 20_000, NOW, WINDOW)).toBe("cool");
  });

  it("윈도우_경계는_cool이다", () => {
    expect(classifyHot(NOW - WINDOW, NOW, WINDOW)).toBe("cool");
    expect(classifyHot(NOW - WINDOW + 1, NOW, WINDOW)).toBe("warm");
  });

  it("미래_mtime은_시계_틀어짐으로_보고_fresh로_잡는다", () => {
    expect(classifyHot(NOW + 30_000, NOW, WINDOW)).toBe("fresh");
  });

  it("mtime이_없으면_cool이다", () => {
    expect(classifyHot(undefined, NOW, WINDOW)).toBe("cool");
  });
});

describe("anyHot", () => {
  it("하나라도_윈도우_안이면_참을_반환한다", () => {
    expect(anyHot([undefined, NOW - 50_000, NOW - 2_000], NOW, WINDOW)).toBe(
      true,
    );
  });

  it("전부_식었거나_비어_있으면_거짓을_반환한다", () => {
    expect(anyHot([undefined, NOW - 50_000], NOW, WINDOW)).toBe(false);
    expect(anyHot([], NOW, WINDOW)).toBe(false);
  });
});

describe("clockOffset", () => {
  it("서버가_앞서면_양수_뒤처지면_음수를_반환한다", () => {
    expect(clockOffset(NOW + 30_000, NOW)).toBe(30_000);
    expect(clockOffset(NOW - 30_000, NOW)).toBe(-30_000);
    expect(clockOffset(NOW, NOW)).toBe(0);
  });

  it("서버_시각이_없거나_유효하지_않으면_보정하지_않는다", () => {
    expect(clockOffset(undefined, NOW)).toBe(0);
    expect(clockOffset(0, NOW)).toBe(0);
  });

  it("보정하면_느린_시계에서도_같은_단계가_나온다", () => {
    // 기기가 30초 느리다: 보정 전에는 window 밖의 파일까지 fresh로 보인다.
    const behind = NOW - 30_000;
    expect(classifyHot(NOW - 20_000, behind, WINDOW)).toBe("fresh");
    const offset = clockOffset(NOW, behind);
    expect(classifyHot(NOW - 20_000, behind + offset, WINDOW)).toBe("cool");
  });

  it("보정하면_빠른_시계에서도_강조가_살아난다", () => {
    // 기기가 30초 빠르다: 보정 전에는 방금 만진 파일이 이미 식어 보인다.
    const ahead = NOW + 30_000;
    expect(classifyHot(NOW - 1_000, ahead, WINDOW)).toBe("cool");
    const offset = clockOffset(NOW, ahead);
    expect(classifyHot(NOW - 1_000, ahead + offset, WINDOW)).toBe("fresh");
  });
});
