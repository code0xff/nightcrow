import { describe, expect, it } from "vitest";
import { paneAt, reconcileOrder, reorderByDrop, swapOrder } from "./paneOrder";

describe("reorderByDrop", () => {
  it("dragging backward inserts before the target", () => {
    // Drag 3 backward onto 1: 3 lands before 1, 1 and 2 shift back.
    expect(reorderByDrop([1, 2, 3], 3, 1)).toEqual([3, 1, 2]);
  });

  it("dragging forward inserts after the target", () => {
    // Drag 1 forward onto the last pane: it goes to the very end.
    expect(reorderByDrop([1, 2, 3], 1, 3)).toEqual([2, 3, 1]);
  });

  it("dropping swaps two neighboring panes", () => {
    expect(reorderByDrop([1, 2, 3], 1, 2)).toEqual([2, 1, 3]);
    expect(reorderByDrop([1, 2, 3], 3, 2)).toEqual([1, 3, 2]);
  });

  it("dropping onto itself preserves the order", () => {
    expect(reorderByDrop([1, 2, 3], 2, 2)).toEqual([1, 2, 3]);
  });

  it("preserves the order when a missing pane is referenced", () => {
    expect(reorderByDrop([1, 2, 3], 9, 1)).toEqual([1, 2, 3]);
    expect(reorderByDrop([1, 2, 3], 1, 9)).toEqual([1, 2, 3]);
  });
});

describe("reconcileOrder", () => {
  it("uses a complete permutation unchanged", () => {
    expect(reconcileOrder([1, 2, 3], [3, 1, 2])).toEqual([3, 1, 2]);
  });

  it("appends omitted panes in their current order", () => {
    expect(reconcileOrder([1, 2, 3], [3])).toEqual([3, 1, 2]);
  });

  it("discards ids that do not exist", () => {
    expect(reconcileOrder([1, 2], [9, 2, 1])).toEqual([2, 1]);
  });

  it("uses duplicate ids once to preserve the permutation", () => {
    expect(reconcileOrder([1, 2], [2, 2, 1])).toEqual([2, 1]);
  });

  it("preserves the current order for an empty request", () => {
    expect(reconcileOrder([1, 2], [])).toEqual([1, 2]);
  });
});

// The same logic backs the project tabs, whose ids are strings — exercise the
// generic with strings so a number-only regression cannot slip through.
describe("also works with string ids (project tabs)", () => {
  it("reorderByDrop reorders string ids", () => {
    expect(reorderByDrop(["r1", "r2", "r3"], "r3", "r1")).toEqual([
      "r3",
      "r1",
      "r2",
    ]);
  });

  it("reconcileOrder reconciles string membership", () => {
    // A repo closed elsewhere ("r2") drops out; a new one ("r4") appends.
    expect(reconcileOrder(["r1", "r3", "r4"], ["r3", "r1", "r2"])).toEqual([
      "r3",
      "r1",
      "r4",
    ]);
  });
});

describe("paneAt select pane by number", () => {
  it("returns the nth pane in visible order", () => {
    // Pane ids are handed out by the session; a shortcut digit names a position.
    expect(paneAt([7, 8, 9], 1)).toBe(7);
    expect(paneAt([7, 8, 9], 3)).toBe(9);
  });

  it("returns null for a missing position", () => {
    expect(paneAt([7, 8, 9], 4)).toBeNull();
    expect(paneAt([], 1)).toBeNull();
    expect(paneAt([7], 0)).toBeNull();
  });
});

describe("swapOrder swap two pane positions", () => {
  it("swaps two positions and leaves the rest unchanged", () => {
    expect(swapOrder([7, 8, 9], 8, 7)).toEqual([8, 7, 9]);
    expect(swapOrder([7, 8, 9, 10], 7, 10)).toEqual([10, 8, 9, 7]);
  });

  it("preserves the layout for a missing pane or the same pane", () => {
    // A stale digit must not be able to rewrite the arrangement.
    expect(swapOrder([7, 8, 9], 8, 99)).toEqual([7, 8, 9]);
    expect(swapOrder([7, 8, 9], 8, 8)).toEqual([7, 8, 9]);
  });

  it("does not mutate the original", () => {
    const order = [7, 8, 9];
    swapOrder(order, 7, 9);
    expect(order).toEqual([7, 8, 9]);
  });
});
