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
  it("shows the bottom when the cursor is at the last row, as before", () => {
    expect(cursorWindowShift(win({ cursorRow: 23 }))).toBe(0);
  });

  it("a new terminal shows the top row containing the prompt", () => {
    // The whole overflow goes below the edge: the top rows are in view.
    expect(cursorWindowShift(win({ cursorRow: 0 }))).toBe(280);
  });

  it("a middle cursor uses the lowest window that still shows its row", () => {
    // Row 14 ends at 300px; a 200px window ending there starts at 100px.
    expect(cursorWindowShift(win({ cursorRow: 14 }))).toBe(180);
  });

  it("does nothing when the pane is larger than the terminal", () => {
    expect(cursorWindowShift(win({ bodyHeight: 480, cursorRow: 0 }))).toBe(0);
    expect(cursorWindowShift(win({ bodyHeight: 600, cursorRow: 0 }))).toBe(0);
  });

  it("does nothing while viewing scrollback", () => {
    expect(cursorWindowShift(win({ cursorRow: 0, atLive: false }))).toBe(0);
  });

  it("clamps an out-of-range cursor to the end", () => {
    expect(cursorWindowShift(win({ cursorRow: 99 }))).toBe(0);
    expect(cursorWindowShift(win({ cursorRow: -3 }))).toBe(280);
    expect(cursorWindowShift(win({ rows: 0 }))).toBe(0);
  });
});
