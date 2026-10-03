import { describe, expect, it } from "vitest";
import { cursorWindowShift, type CursorWindow } from "./cursorWindow";

// 24 rows of 20px in a 200px pane: 280px of terminal out of sight.
function win(over: Partial<CursorWindow> = {}): CursorWindow {
  return {
    termHeight: 480,
    bodyHeight: 200,
    rows: 24,
    cursorRow: 23,
    atLive: true,
    ...over,
  };
}

describe("cursorWindowShift", () => {
  it("커서가_맨_아래면_지금처럼_아래쪽을_보인다", () => {
    expect(cursorWindowShift(win({ cursorRow: 23 }))).toBe(0);
  });

  it("새_터미널은_프롬프트가_있는_맨_위를_보인다", () => {
    // The whole overflow goes below the edge: the top rows are in view.
    expect(cursorWindowShift(win({ cursorRow: 0 }))).toBe(280);
  });

  it("중간의_커서는_그_줄이_보이는_가장_낮은_창이다", () => {
    // Row 14 ends at 300px; a 200px window ending there starts at 100px.
    expect(cursorWindowShift(win({ cursorRow: 14 }))).toBe(180);
  });

  it("pane이_터미널보다_크면_건드리지_않는다", () => {
    expect(cursorWindowShift(win({ bodyHeight: 480, cursorRow: 0 }))).toBe(0);
    expect(cursorWindowShift(win({ bodyHeight: 600, cursorRow: 0 }))).toBe(0);
  });

  it("스크롤백을_보고_있으면_건드리지_않는다", () => {
    expect(cursorWindowShift(win({ cursorRow: 0, atLive: false }))).toBe(0);
  });

  it("범위를_벗어난_커서는_끝으로_자른다", () => {
    expect(cursorWindowShift(win({ cursorRow: 99 }))).toBe(0);
    expect(cursorWindowShift(win({ cursorRow: -3 }))).toBe(280);
    expect(cursorWindowShift(win({ rows: 0 }))).toBe(0);
  });
});
