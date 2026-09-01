// @vitest-environment happy-dom

import { cleanup } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import {
  el,
  mount,
  mounted,
  pane,
  press,
  type Init,
} from "./useProjectCycleShortcut.harness";

afterEach(() => {
  cleanup();
  document.body.innerHTML = "";
});

describe("useProjectCycleShortcut 화살표를 넘겨주는 경우", () => {
  it("수식키가_정확히_맞지_않으면_그대로_통과시킨다", () => {
    const cases: Init[] = [
      { metaKey: true },
      { altKey: true },
      { ctrlKey: false },
      { shiftKey: false },
    ];
    for (const extra of cases) {
      const selectRepo = mount();
      const event = press(document.body, { key: "ArrowLeft", ...extra });
      expect(selectRepo).not.toHaveBeenCalled();
      expect(event.defaultPrevented).toBe(false);
      cleanup();
    }
  });

  it("화살표가_아닌_키는_통과시킨다", () => {
    const selectRepo = mount();
    const event = press(document.body, { key: "ArrowUp" });
    expect(selectRepo).not.toHaveBeenCalled();
    expect(event.defaultPrevented).toBe(false);
  });

  it("자동_반복은_무시한다", () => {
    const selectRepo = mount();
    const event = press(document.body, { key: "ArrowRight", repeat: true });
    expect(selectRepo).not.toHaveBeenCalled();
    expect(event.defaultPrevented).toBe(false);
  });

  it("조합_중인_입력은_명령이_아니다", () => {
    const cases: Init[] = [
      { key: "ArrowRight", isComposing: true },
      { key: "ArrowRight", keyCode: 229 },
      { key: "Process" },
      { key: "Unidentified" },
    ];
    for (const init of cases) {
      const selectRepo = mount();
      const event = press(document.body, init);
      expect(selectRepo).not.toHaveBeenCalled();
      expect(event.defaultPrevented).toBe(false);
      cleanup();
    }
  });

  it("텍스트_입력_안에서는_단어_선택_제스처를_건드리지_않는다", () => {
    const roles = ["textbox", "searchbox", "combobox"];
    const targets = [
      el("input", { type: "text" }),
      el("textarea"),
      el("select"),
      el("div", { contenteditable: "true" }),
      ...roles.map((role) => el("div", { role })),
    ];
    for (const node of targets) {
      const selectRepo = mount();
      const event = press(mounted(node), { key: "ArrowRight" });
      expect(selectRepo, node.outerHTML).not.toHaveBeenCalled();
      expect(event.defaultPrevented, node.outerHTML).toBe(false);
      cleanup();
      document.body.innerHTML = "";
    }
  });

  it("열린_다이얼로그_안에서는_화살표를_다이얼로그에_맡긴다", () => {
    const dialog = mounted(el("div", { role: "dialog" }));
    const button = dialog.appendChild(el("button"));
    const selectRepo = mount();

    const event = press(button, { key: "ArrowRight" });

    expect(selectRepo).not.toHaveBeenCalled();
    expect(event.defaultPrevented).toBe(false);
  });

  it("터미널_패널_안에서는_한_번만_실행되고_PTY로_한_바이트도_가지_않는다", () => {
    const { xterm, xtermKeydown } = pane();
    const selectRepo = mount();

    const event = press(xterm, { key: "ArrowRight" });

    expect(xtermKeydown).not.toHaveBeenCalled();
    expect(selectRepo).toHaveBeenCalledTimes(1);
    expect(selectRepo).toHaveBeenCalledWith("c");
    expect(event.defaultPrevented).toBe(true);
  });

  it("터미널_패널_안의_다른_키는_xterm까지_그대로_간다", () => {
    const { xterm, xtermKeydown } = pane();
    mount();

    press(xterm, { key: "ArrowUp" });

    expect(xtermKeydown).toHaveBeenCalledTimes(1);
  });
});
