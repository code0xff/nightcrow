// @vitest-environment happy-dom
//
// Every page command reaches the control the button already calls. That is the
// point of the registry: a keyboard that reimplemented "close the project" would
// drift from the tab's close button the first time either changed.

import { act, cleanup } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import type { Maximized } from "../../types";
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

describe("useAppShortcuts page commands", () => {
  it("project commands call the existing controls", () => {
    const { openPicker, closeRepo } = mount();

    command("o");
    command("x");

    expect(openPicker).toHaveBeenCalledTimes(1);
    expect(closeRepo).toHaveBeenCalledWith("b");
  });

  it("session commands call the existing controls", () => {
    const { cycleAccent, reloadConfig } = mount();

    command("p");
    command("u");

    expect(cycleAccent).toHaveBeenCalledTimes(1);
    expect(reloadConfig).toHaveBeenCalledTimes(1);
  });

  it("l toggles between status and commit log", () => {
    const fromStatus = mount({ tab: "status" });
    command("l");
    expect(fromStatus.chooseTab).toHaveBeenCalledWith("log");
    cleanup();

    const fromLog = mount({ tab: "log" });
    command("l");
    expect(fromLog.chooseTab).toHaveBeenCalledWith("status");
  });

  it("b toggles the tree view", () => {
    const fromStatus = mount({ tab: "status" });
    command("b");
    expect(fromStatus.chooseTab).toHaveBeenCalledWith("tree");
    cleanup();

    const fromTree = mount({ tab: "tree" });
    command("b");
    expect(fromTree.chooseTab).toHaveBeenCalledWith("status");
  });

  it("f maximizes the panel with keyboard focus", () => {
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

  it("f maximizes the file panel when focus is outside a panel", () => {
    const { setMaximized, zoomActivePane } = mount();

    command("f");

    expect(maximizedFrom(setMaximized, "none")).toBe("files");
    expect(maximizedFrom(setMaximized, "files")).toBe("none");
    expect(zoomActivePane).not.toHaveBeenCalled();
  });

  it("1 and 2 focus the corresponding visible regions", () => {
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

  it("focus commands take the keyboard from the terminal panel", () => {
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

  it("? opens help and does not consume other keys while it is open", () => {
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

  it("closing help returns the keyboard", () => {
    const { help, openPicker } = mount();

    command("?");
    act(() => help.current!.hide());
    command("o");

    expect(openPicker).toHaveBeenCalledTimes(1);
  });

  it("project-only commands are unavailable when there is no project", () => {
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

  it("page commands are available when a project is open", () => {
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

  it("delegates keys to the dialog while the folder picker is open", () => {
    const { openPicker } = mount({ pickerOpen: true });

    const armed = hit({ key: "f", ctrlKey: true });
    hit({ key: "o" });

    expect(armed.defaultPrevented).toBe(false);
    expect(openPicker).not.toHaveBeenCalled();
  });
});
