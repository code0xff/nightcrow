import { describe, expect, it } from "vitest";
import { anyHot, classifyHot } from "./hot";

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
