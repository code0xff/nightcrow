import { describe, expect, it } from "vitest";
import { neighborRepo } from "./projectCycle";

describe("neighborRepo", () => {
  it("다음_프로젝트는_순서상_바로_뒤다", () => {
    expect(neighborRepo(["a", "b", "c"], "a", 1)).toBe("b");
    expect(neighborRepo(["a", "b", "c"], "b", 1)).toBe("c");
  });

  it("이전_프로젝트는_순서상_바로_앞이다", () => {
    expect(neighborRepo(["a", "b", "c"], "c", -1)).toBe("b");
    expect(neighborRepo(["a", "b", "c"], "b", -1)).toBe("a");
  });

  it("마지막에서_다음은_처음으로_감싼다", () => {
    expect(neighborRepo(["a", "b", "c"], "c", 1)).toBe("a");
  });

  it("처음에서_이전은_마지막으로_감싼다", () => {
    expect(neighborRepo(["a", "b", "c"], "a", -1)).toBe("c");
  });

  it("두_개면_양방향_모두_상대편이다", () => {
    expect(neighborRepo(["a", "b"], "a", 1)).toBe("b");
    expect(neighborRepo(["a", "b"], "a", -1)).toBe("b");
    expect(neighborRepo(["a", "b"], "b", 1)).toBe("a");
    expect(neighborRepo(["a", "b"], "b", -1)).toBe("a");
  });

  it("하나뿐이면_옮겨갈_곳이_없다", () => {
    expect(neighborRepo(["a"], "a", 1)).toBeNull();
    expect(neighborRepo(["a"], "a", -1)).toBeNull();
  });

  it("빈_목록이면_옮겨갈_곳이_없다", () => {
    expect(neighborRepo([], null, 1)).toBeNull();
    expect(neighborRepo([], "a", -1)).toBeNull();
  });

  it("선택이_없으면_아무것도_하지_않는다", () => {
    expect(neighborRepo(["a", "b"], null, 1)).toBeNull();
  });

  it("목록에_없는_선택이면_아무것도_하지_않는다", () => {
    // 폴링이 아직 정리하지 못한 상태다. 여기서 첫 탭으로 뛰면 그 결정과 다툰다.
    expect(neighborRepo(["a", "b"], "gone", 1)).toBeNull();
    expect(neighborRepo(["a", "b"], "gone", -1)).toBeNull();
  });

  it("중복이_있으면_첫_등장을_기준으로_센다", () => {
    // 탭 목록은 중복 없는 id를 주지만, 계산이 무엇을 하는지는 고정해 둔다.
    expect(neighborRepo(["a", "b", "a"], "a", 1)).toBe("b");
  });
});
