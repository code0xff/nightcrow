import { describe, expect, it } from "vitest";
import { paneAt, reconcileOrder, reorderByDrop, swapOrder } from "./paneOrder";

describe("reorderByDrop", () => {
  it("뒤로_끌면_타겟_앞에_삽입한다", () => {
    // Drag 3 backward onto 1: 3 lands before 1, 1 and 2 shift back.
    expect(reorderByDrop([1, 2, 3], 3, 1)).toEqual([3, 1, 2]);
  });

  it("앞으로_끌면_타겟_뒤에_삽입한다", () => {
    // Drag 1 forward onto the last pane: it goes to the very end.
    expect(reorderByDrop([1, 2, 3], 1, 3)).toEqual([2, 3, 1]);
  });

  it("이웃한_두_pane은_드롭으로_자리를_바꾼다", () => {
    expect(reorderByDrop([1, 2, 3], 1, 2)).toEqual([2, 1, 3]);
    expect(reorderByDrop([1, 2, 3], 3, 2)).toEqual([1, 3, 2]);
  });

  it("자기_자신에게_드롭하면_순서가_그대로다", () => {
    expect(reorderByDrop([1, 2, 3], 2, 2)).toEqual([1, 2, 3]);
  });

  it("없는_pane을_참조하면_순서가_그대로다", () => {
    expect(reorderByDrop([1, 2, 3], 9, 1)).toEqual([1, 2, 3]);
    expect(reorderByDrop([1, 2, 3], 1, 9)).toEqual([1, 2, 3]);
  });
});

describe("reconcileOrder", () => {
  it("완전한_순열은_그대로_따른다", () => {
    expect(reconcileOrder([1, 2, 3], [3, 1, 2])).toEqual([3, 1, 2]);
  });

  it("빠진_pane은_현재_순서로_뒤에_붙인다", () => {
    expect(reconcileOrder([1, 2, 3], [3])).toEqual([3, 1, 2]);
  });

  it("존재하지_않는_id는_버린다", () => {
    expect(reconcileOrder([1, 2], [9, 2, 1])).toEqual([2, 1]);
  });

  it("중복_id는_한_번만_취해_순열을_유지한다", () => {
    expect(reconcileOrder([1, 2], [2, 2, 1])).toEqual([2, 1]);
  });

  it("빈_요청이면_현재_순서를_유지한다", () => {
    expect(reconcileOrder([1, 2], [])).toEqual([1, 2]);
  });
});

// The same logic backs the project tabs, whose ids are strings — exercise the
// generic with strings so a number-only regression cannot slip through.
describe("문자열_id로도_동작한다 (project tabs)", () => {
  it("reorderByDrop이_문자열_id를_재배열한다", () => {
    expect(reorderByDrop(["r1", "r2", "r3"], "r3", "r1")).toEqual([
      "r3",
      "r1",
      "r2",
    ]);
  });

  it("reconcileOrder가_문자열_membership을_접는다", () => {
    // A repo closed elsewhere ("r2") drops out; a new one ("r4") appends.
    expect(reconcileOrder(["r1", "r3", "r4"], ["r3", "r1", "r2"])).toEqual([
      "r3",
      "r1",
      "r4",
    ]);
  });
});

describe("paneAt 숫자로 pane 고르기", () => {
  it("보이는_순서의_n번째를_돌려준다", () => {
    // Pane ids are handed out by the session; a shortcut digit names a position.
    expect(paneAt([7, 8, 9], 1)).toBe(7);
    expect(paneAt([7, 8, 9], 3)).toBe(9);
  });

  it("없는_자리는_null이다", () => {
    expect(paneAt([7, 8, 9], 4)).toBeNull();
    expect(paneAt([], 1)).toBeNull();
    expect(paneAt([7], 0)).toBeNull();
  });
});

describe("swapOrder 두 pane 자리 바꾸기", () => {
  it("두_자리를_맞바꾸고_나머지는_두지_않는다", () => {
    expect(swapOrder([7, 8, 9], 8, 7)).toEqual([8, 7, 9]);
    expect(swapOrder([7, 8, 9, 10], 7, 10)).toEqual([10, 8, 9, 7]);
  });

  it("없는_pane이나_같은_pane이면_배치는_그대로다", () => {
    // A stale digit must not be able to rewrite the arrangement.
    expect(swapOrder([7, 8, 9], 8, 99)).toEqual([7, 8, 9]);
    expect(swapOrder([7, 8, 9], 8, 8)).toEqual([7, 8, 9]);
  });

  it("원본을_바꾸지_않는다", () => {
    const order = [7, 8, 9];
    swapOrder(order, 7, 9);
    expect(order).toEqual([7, 8, 9]);
  });
});
