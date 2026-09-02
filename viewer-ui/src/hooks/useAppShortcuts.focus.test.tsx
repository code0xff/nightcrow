// @vitest-environment happy-dom
//
// `Shift+Left` / `Shift+Right`, the TUI's focus ring on the web. What is walked
// is `lib/focusCycle.ts`'s business and tested there; this covers the wiring —
// that the DOM region and the panel's cursor are read, that a region is focused
// through the same door `<prefix> 1` uses, that a pane goes through the panel,
// and that the key is consumed whether or not anything moved.

import { cleanup } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { focusRegionAttrs } from "../lib/shortcutDom";
import { hit, mount } from "./useAppShortcuts.harness";
import type { Init } from "./useShortcuts.harness";

const NEXT: Init = { key: "ArrowRight", shiftKey: true };
const PREVIOUS: Init = { key: "ArrowLeft", shiftKey: true };

afterEach(() => {
  cleanup();
  document.body.innerHTML = "";
});

/** The two upper regions as the page marks them up. */
function regions() {
  const list = document.createElement("div");
  const content = document.createElement("div");
  for (const [node, region] of [
    [list, "list"],
    [content, "content"],
  ] as const) {
    const attrs = focusRegionAttrs(region);
    node.setAttribute("data-focus-region", attrs["data-focus-region"]);
    node.tabIndex = attrs.tabIndex;
    document.body.appendChild(node);
  }
  return { list, content };
}

/** A panel with `count` panes, the keyboard on the pane at `index`. */
function panel(count: number, index: number) {
  const node = document.createElement("div");
  node.setAttribute("data-terminal-panel", "");
  const xterm = node.appendChild(document.createElement("textarea"));
  document.body.appendChild(node);
  const focusPaneAt = vi.fn();
  return {
    xterm,
    focusPaneAt,
    handlers: { paneCursor: () => ({ index, count }), focusPaneAt },
  };
}

describe("useAppShortcuts 포커스 순환", () => {
  it("목록에서_오른쪽은_콘텐츠로_간다", () => {
    const { list, content } = regions();
    mount();
    list.focus();

    hit(NEXT, list);

    expect(document.activeElement).toBe(content);
  });

  it("콘텐츠에서_오른쪽은_첫_pane으로_패널을_통해_간다", () => {
    const { content } = regions();
    const term = panel(2, -1);
    mount({}, term.handlers);
    content.focus();

    hit(NEXT, content);

    expect(term.focusPaneAt).toHaveBeenCalledWith(0);
  });

  it("pane에서_오른쪽은_다음_pane이다", () => {
    regions();
    const term = panel(3, 0);
    mount({}, term.handlers);
    term.xterm.focus();

    hit(NEXT, term.xterm);

    expect(term.focusPaneAt).toHaveBeenCalledWith(1);
  });

  it("마지막_pane에서_오른쪽은_목록으로_돌아온다", () => {
    const { list } = regions();
    const term = panel(2, 1);
    mount({}, term.handlers);
    term.xterm.focus();

    hit(NEXT, term.xterm);

    expect(document.activeElement).toBe(list);
    expect(term.focusPaneAt).not.toHaveBeenCalled();
  });

  it("목록에서_왼쪽은_마지막_pane이다", () => {
    const { list } = regions();
    const term = panel(3, -1);
    mount({}, term.handlers);
    list.focus();

    hit(PREVIOUS, list);

    expect(term.focusPaneAt).toHaveBeenCalledWith(2);
  });

  it("터미널이_최대화되면_pane_사이만_감싸며_돈다", () => {
    const { list } = regions();
    const term = panel(2, 1);
    mount({ maximized: "terminal" }, term.handlers);
    term.xterm.focus();

    hit(NEXT, term.xterm);

    expect(term.focusPaneAt).toHaveBeenCalledWith(0);
    expect(document.activeElement).not.toBe(list);
  });

  it("패널이_없어도_목록과_콘텐츠는_오간다", () => {
    const { list, content } = regions();
    mount();
    content.focus();

    hit(NEXT, content);

    expect(document.activeElement).toBe(list);
  });

  it("갈_곳이_없어도_키는_먹어서_pane에_새지_않는다", () => {
    // One pane, terminal maximized: the ring is a single spot. The chord is
    // still the page's, or `ESC[1;2C` would reach the shell.
    const term = panel(1, 0);
    mount({ maximized: "terminal" }, term.handlers);
    term.xterm.focus();

    const event = hit(NEXT, term.xterm);

    expect(event.defaultPrevented).toBe(true);
    expect(term.focusPaneAt).not.toHaveBeenCalled();
  });

  it("프로젝트가_없으면_순환은_가용하지_않다", () => {
    const { bus } = mount({ repo: null, repos: [] });

    expect(bus.current?.isAvailable("focus.next")).toBe(false);
    expect(bus.current?.isAvailable("focus.previous")).toBe(false);
  });

  it("텍스트_필드에서는_명령이_아니다", () => {
    // Shift+Arrow in a field is the browser's word selection; the classifier
    // suppresses everything there, and the field keeps its key.
    regions();
    const input = document.body.appendChild(document.createElement("input"));
    mount();
    input.focus();

    const event = hit(NEXT, input);

    expect(event.defaultPrevented).toBe(false);
    expect(document.activeElement).toBe(input);
  });
});

describe("useAppShortcuts 포커스 순환 좁은 화면", () => {
  const wide = window.innerWidth;
  afterEach(() => {
    Object.defineProperty(window, "innerWidth", { value: wide, configurable: true });
  });

  it("보이는_뷰가_아닌_곳으로는_가지_않는다", () => {
    // A phone with a keyboard, showing the content view: the list and the
    // panes are hidden, and the key must not move the keyboard — or the panel's
    // active pane — somewhere unseen.
    Object.defineProperty(window, "innerWidth", { value: 500, configurable: true });
    const { list, content } = regions();
    const term = panel(2, -1);
    mount({ mobileView: "diff" }, term.handlers);
    content.focus();

    const event = hit(NEXT, content);

    expect(document.activeElement).toBe(content);
    expect(document.activeElement).not.toBe(list);
    expect(term.focusPaneAt).not.toHaveBeenCalled();
    expect(event.defaultPrevented).toBe(true);
  });

  it("터미널_뷰에서는_pane_사이를_돈다", () => {
    Object.defineProperty(window, "innerWidth", { value: 500, configurable: true });
    regions();
    const term = panel(2, 1);
    mount({ mobileView: "terminal" }, term.handlers);
    term.xterm.focus();

    hit(NEXT, term.xterm);

    expect(term.focusPaneAt).toHaveBeenCalledWith(0);
  });
});
