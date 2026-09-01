// @vitest-environment happy-dom

import { cleanup } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import { mount, press } from "./useProjectCycleShortcut.harness";

afterEach(cleanup);

describe("useProjectCycleShortcut 프로젝트 순환", () => {
  it("오른쪽_화살표는_다음_프로젝트로_간다", () => {
    const selectRepo = mount();
    press(document.body, { key: "ArrowRight" });
    expect(selectRepo).toHaveBeenCalledWith("c");
  });

  it("왼쪽_화살표는_이전_프로젝트로_간다", () => {
    const selectRepo = mount();
    press(document.body, { key: "ArrowLeft" });
    expect(selectRepo).toHaveBeenCalledWith("a");
  });

  it("마지막에서_오른쪽은_처음으로_감싼다", () => {
    const selectRepo = mount({ repo: "c" });
    press(document.body, { key: "ArrowRight" });
    expect(selectRepo).toHaveBeenCalledWith("a");
  });

  it("처음에서_왼쪽은_마지막으로_감싼다", () => {
    const selectRepo = mount({ repo: "a" });
    press(document.body, { key: "ArrowLeft" });
    expect(selectRepo).toHaveBeenCalledWith("c");
  });

  it("한_번_눌러_한_번만_전환한다", () => {
    const selectRepo = mount();
    press(document.body, { key: "ArrowRight" });
    expect(selectRepo).toHaveBeenCalledTimes(1);
  });

  it("프로젝트가_없거나_하나면_아무_전환도_없지만_키는_먹는다", () => {
    // 예약된 화살표가 1개 프로젝트일 때만 셸로 새는 일은 없어야 한다.
    for (const args of [
      { repos: [], repo: null },
      { repos: [{ id: "a" }], repo: "a" },
    ]) {
      const selectRepo = mount(args);
      const event = press(document.body, { key: "ArrowRight" });
      expect(selectRepo).not.toHaveBeenCalled();
      expect(event.defaultPrevented).toBe(true);
      cleanup();
    }
  });

  it("목록에_없는_선택이면_전환하지_않는다", () => {
    const selectRepo = mount({ repo: "gone" });
    press(document.body, { key: "ArrowRight" });
    expect(selectRepo).not.toHaveBeenCalled();
  });

  it("enabled가_false면_동작하지_않는다", () => {
    const selectRepo = mount({ enabled: false });
    const event = press(document.body, { key: "ArrowRight" });
    expect(selectRepo).not.toHaveBeenCalled();
    expect(event.defaultPrevented).toBe(false);
  });
});
