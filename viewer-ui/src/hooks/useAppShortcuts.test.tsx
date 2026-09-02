// @vitest-environment happy-dom
//
// Every page command reaches the control the button already calls. That is the
// point of the registry: a keyboard that reimplemented "close the project" would
// drift from the tab's close button the first time either changed.

import { act, cleanup } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import type { Maximized } from "../types";
import { command, hit, mount } from "./useAppShortcuts.harness";
import { el, mounted, pane } from "./useShortcuts.harness";

afterEach(() => {
  cleanup();
  document.body.innerHTML = "";
});

/** What a `setMaximized` updater would leave the panel at. */
function maximizedFrom(
  spy: { mock: { calls: unknown[][] } },
  previous: Maximized,
): Maximized {
  const updater = spy.mock.calls[0][0] as (from: Maximized) => Maximized;
  return updater(previous);
}

describe("useAppShortcuts 페이지 명령", () => {
  it("프로젝트_명령은_기존_컨트롤을_부른다", () => {
    const { openPicker, closeRepo } = mount();

    command("o");
    command("x");

    expect(openPicker).toHaveBeenCalledTimes(1);
    expect(closeRepo).toHaveBeenCalledWith("b");
  });

  it("세션_명령은_기존_컨트롤을_부른다", () => {
    const { cycleAccent, reloadConfig } = mount();

    command("p");
    command("u");

    expect(cycleAccent).toHaveBeenCalledTimes(1);
    expect(reloadConfig).toHaveBeenCalledTimes(1);
  });

  it("l은_상태와_커밋_로그를_왕복한다", () => {
    const fromStatus = mount({ tab: "status" });
    command("l");
    expect(fromStatus.chooseTab).toHaveBeenCalledWith("log");
    cleanup();

    const fromLog = mount({ tab: "log" });
    command("l");
    expect(fromLog.chooseTab).toHaveBeenCalledWith("status");
  });

  it("b는_트리_뷰를_왕복한다", () => {
    const fromStatus = mount({ tab: "status" });
    command("b");
    expect(fromStatus.chooseTab).toHaveBeenCalledWith("tree");
    cleanup();

    const fromTree = mount({ tab: "tree" });
    command("b");
    expect(fromTree.chooseTab).toHaveBeenCalledWith("status");
  });

  it("f는_키보드가_있는_패널을_최대화한다", () => {
    // The reinterpreted maximize: the panel with the keyboard, plus the zoom on
    // the active pane. Never `requestFullscreen`, never F11.
    const { xterm } = pane();
    const { setMaximized, zoomActivePane } = mount();
    xterm.focus();

    command("f", xterm);

    expect(maximizedFrom(setMaximized, "none")).toBe("terminal");
    expect(maximizedFrom(setMaximized, "terminal")).toBe("none");
    expect(zoomActivePane).toHaveBeenCalledTimes(1);
  });

  it("f는_패널_밖에서는_파일_패널을_최대화한다", () => {
    const { setMaximized, zoomActivePane } = mount();

    command("f");

    expect(maximizedFrom(setMaximized, "none")).toBe("files");
    expect(maximizedFrom(setMaximized, "files")).toBe("none");
    expect(zoomActivePane).not.toHaveBeenCalled();
  });

  it("1과_2는_표시된_영역으로_포커스를_옮긴다", () => {
    const list = mounted(
      el("section", { "data-focus-region": "list", tabindex: "-1" }),
    );
    const content = mounted(
      el("section", { "data-focus-region": "content", tabindex: "-1" }),
    );
    mount();

    command("1");
    expect(document.activeElement).toBe(list);

    command("2");
    expect(document.activeElement).toBe(content);
  });

  it("포커스_명령은_터미널_패널에서_키보드를_가져온다", () => {
    // And is not undone: the panel re-asserts pane focus on layout signals
    // (`usePaneFocus`), and a focus command is not one of them.
    const { xterm } = pane();
    const list = mounted(
      el("section", { "data-focus-region": "list", tabindex: "-1" }),
    );
    mount();
    xterm.focus();

    command("1", xterm);

    expect(document.activeElement).toBe(list);
  });

  it("물음표는_도움말을_열고_열려_있는_동안_다른_키를_먹지_않는다", () => {
    const { help, openPicker } = mount();
    expect(help.current?.open).toBe(false);

    command("?");
    expect(help.current?.open).toBe(true);

    // The sheet is modal, so the leader must not fire underneath it.
    const armed = hit({ key: "f", ctrlKey: true });
    expect(armed.defaultPrevented).toBe(false);
    hit({ key: "o" });
    expect(openPicker).not.toHaveBeenCalled();
  });

  it("도움말은_닫으면_다시_키보드를_돌려준다", () => {
    const { help, openPicker } = mount();

    command("?");
    act(() => help.current!.hide());
    command("o");

    expect(openPicker).toHaveBeenCalledTimes(1);
  });

  it("프로젝트가_없으면_프로젝트_전용_명령은_가용하지_않다", () => {
    const { bus } = mount({ repo: null, repos: [] });
    const available = bus.current!.isAvailable;

    for (const id of [
      "project.close",
      "view.toggleLog",
      "view.toggleTree",
      "view.toggleMaximize",
      "focus.list",
      "focus.content",
    ] as const) {
      expect(available(id), id).toBe(false);
    }
    // Opening one, cycling the accent and the help sheet always are.
    expect(available("project.openDialog")).toBe(true);
    expect(available("session.cycleAccent")).toBe(true);
    expect(available("help.shortcuts")).toBe(true);
  });

  it("프로젝트가_열려_있으면_화면_명령이_가용하다", () => {
    const { bus } = mount();
    const available = bus.current!.isAvailable;

    for (const id of [
      "project.close",
      "view.toggleLog",
      "view.toggleTree",
      "view.toggleMaximize",
      "focus.list",
      "focus.content",
    ] as const) {
      expect(available(id), id).toBe(true);
    }
  });

  it("폴더_고르기가_열려_있으면_키를_다이얼로그에_맡긴다", () => {
    const { openPicker } = mount({ pickerOpen: true });

    const armed = hit({ key: "f", ctrlKey: true });
    hit({ key: "o" });

    expect(armed.defaultPrevented).toBe(false);
    expect(openPicker).not.toHaveBeenCalled();
  });
});
