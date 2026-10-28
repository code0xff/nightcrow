// @vitest-environment happy-dom
//
// `Shift+Left` / `Shift+Right`, the TUI's focus ring on the web. What is walked
// is `lib/focusCycle.ts`'s business and tested there; this covers the wiring —
// that the DOM region and the panel's cursor are read, that a region is focused
// through the same door `<prefix> 1` uses, that a pane goes through the panel,
// and that the key is consumed whether or not anything moved.

import { cleanup } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { focusRegionAttrs } from "../../lib/shortcuts/shortcutDom";
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

describe("useAppShortcuts focus cycling", () => {
  it("right from the list moves to content", () => {
    const { list, content } = regions();
    mount();
    list.focus();

    hit(NEXT, list);

    expect(document.activeElement).toBe(content);
  });

  it("right from content moves through the panel to the first pane", () => {
    const { content } = regions();
    const term = panel(2, -1);
    mount({}, term.handlers);
    content.focus();

    hit(NEXT, content);

    expect(term.focusPaneAt).toHaveBeenCalledWith(0);
  });

  it("right from a pane moves to the next pane", () => {
    regions();
    const term = panel(3, 0);
    mount({}, term.handlers);
    term.xterm.focus();

    hit(NEXT, term.xterm);

    expect(term.focusPaneAt).toHaveBeenCalledWith(1);
  });

  it("right from the last pane returns to the list", () => {
    const { list } = regions();
    const term = panel(2, 1);
    mount({}, term.handlers);
    term.xterm.focus();

    hit(NEXT, term.xterm);

    expect(document.activeElement).toBe(list);
    expect(term.focusPaneAt).not.toHaveBeenCalled();
  });

  it("left from the list moves to the last pane", () => {
    const { list } = regions();
    const term = panel(3, -1);
    mount({}, term.handlers);
    list.focus();

    hit(PREVIOUS, list);

    expect(term.focusPaneAt).toHaveBeenCalledWith(2);
  });

  it("when the terminal is maximized, cycles only among panes", () => {
    const { list } = regions();
    const term = panel(2, 1);
    mount({ maximized: "terminal" }, term.handlers);
    term.xterm.focus();

    hit(NEXT, term.xterm);

    expect(term.focusPaneAt).toHaveBeenCalledWith(0);
    expect(document.activeElement).not.toBe(list);
  });

  it("list and content still cycle when there is no panel", () => {
    const { list, content } = regions();
    mount();
    content.focus();

    hit(NEXT, content);

    expect(document.activeElement).toBe(list);
  });

  it("consumes the key when there is nowhere to move so it does not leak to the pane", () => {
    // One pane, terminal maximized: the ring is a single spot. The chord is
    // still the page's, or `ESC[1;2C` would reach the shell.
    const term = panel(1, 0);
    mount({ maximized: "terminal" }, term.handlers);
    term.xterm.focus();

    const event = hit(NEXT, term.xterm);

    expect(event.defaultPrevented).toBe(true);
    expect(term.focusPaneAt).not.toHaveBeenCalled();
  });

  it("focus cycling is unavailable when there is no project", () => {
    const { bus } = mount({ repo: null, repos: [] });

    expect(bus.current?.isAvailable("focus.next")).toBe(false);
    expect(bus.current?.isAvailable("focus.previous")).toBe(false);
  });

  it("is not a command in a text field", () => {
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

describe("useAppShortcuts focus cycling on narrow screens", () => {
  const wide = window.innerWidth;
  afterEach(() => {
    Object.defineProperty(window, "innerWidth", { value: wide, configurable: true });
  });

  it("does not move to a view that is not visible", () => {
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

  it("cycles between panes in the terminal view", () => {
    Object.defineProperty(window, "innerWidth", { value: 500, configurable: true });
    regions();
    const term = panel(2, 1);
    mount({ mobileView: "terminal" }, term.handlers);
    term.xterm.focus();

    hit(NEXT, term.xterm);

    expect(term.focusPaneAt).toHaveBeenCalledWith(0);
  });
});
