import { describe, expect, it } from "vitest";
import { lineScrollTop, virtualWindow } from "./virtualWindow";

describe("virtualWindow", () => {
  it("selects only rows within the viewport and overscan", () => {
    expect(virtualWindow(20_000, 4_000, 400, 20, 5)).toEqual({
      start: 195,
      end: 225,
      before: 3_900,
      after: 395_500,
    });
  });

  it("does not exceed the range at the start or end", () => {
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

  it("maps anchor_line to scrollTop using the same row height", () => {
    expect(lineScrollTop(1)).toBe(0);
    expect(lineScrollTop(501)).toBe(10_000);
  });
});
