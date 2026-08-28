import { describe, expect, it } from "vitest";
import { lineScrollTop, virtualWindow } from "./virtualWindow";

describe("virtualWindow", () => {
  it("viewport와_overscan에_해당하는_행만_고른다", () => {
    expect(virtualWindow(20_000, 4_000, 400, 20, 5)).toEqual({
      start: 195,
      end: 225,
      before: 3_900,
      after: 395_500,
    });
  });

  it("처음과_끝에서_범위를_벗어나지_않는다", () => {
    expect(virtualWindow(10, 0, 40, 20, 3)).toEqual({
      start: 0,
      end: 5,
      before: 0,
      after: 100,
    });
    expect(virtualWindow(10, 999, 40, 20, 3)).toEqual({
      start: 5,
      end: 10,
      before: 100,
      after: 0,
    });
  });

  it("anchor_line을_같은_행_높이의_scrollTop으로_옮긴다", () => {
    expect(lineScrollTop(1)).toBe(0);
    expect(lineScrollTop(501)).toBe(10_000);
  });
});
