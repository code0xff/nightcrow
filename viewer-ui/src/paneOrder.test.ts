import { describe, expect, it } from "vitest";
import { reconcileOrder, reorderByDrop } from "./paneOrder";

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
