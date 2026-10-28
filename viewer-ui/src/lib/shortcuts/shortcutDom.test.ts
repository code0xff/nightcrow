// @vitest-environment happy-dom

import { afterEach, describe, expect, it } from "vitest";
import {
  describeShortcutTarget,
  focusRegionAttrs,
  focusShortcutRegion,
  keyboardRegion,
  terminalPanelHasFocus,
} from "./shortcutDom";
import { isTextEntryTarget, shortcutsSuppressed } from "./shortcutTarget";

afterEach(() => {
  document.body.innerHTML = "";
});

function el(tag: string, attrs: Record<string, string> = {}) {
  const node = document.createElement(tag);
  for (const [key, value] of Object.entries(attrs)) node.setAttribute(key, value);
  document.body.appendChild(node);
  return node;
}

function suppressed(target: EventTarget | null): boolean {
  return shortcutsSuppressed({
    target: describeShortcutTarget(target),
    dialogOpen: false,
    composing: false,
  });
}

describe("describeShortcutTarget key target", () => {
  it("inside the terminal panel is not typing", () => {
    // xterm's input surface is a `<textarea>`, which the text-field rule matches
    // by tag alone. Inside the panel the page owns the keyboard.
    const panel = el("div", { "data-terminal-panel": "" });
    const xterm = panel.appendChild(document.createElement("textarea"));

    expect(describeShortcutTarget(xterm)).toBeNull();
    expect(suppressed(xterm)).toBe(false);
  });

  it("a textarea outside the panel is typing", () => {
    expect(suppressed(el("textarea"))).toBe(true);
  });

  it("passes through the input type unchanged", () => {
    expect(describeShortcutTarget(el("input", { type: "CHECKBOX" }))?.type).toBe(
      "checkbox",
    );
    expect(suppressed(el("input", { type: "checkbox" }))).toBe(false);
    expect(suppressed(el("input", { type: "password" }))).toBe(true);
  });

  it("reads the editable ancestor", () => {
    const host = el("div", { contenteditable: "true" });
    const inner = host.appendChild(document.createElement("span"));

    expect(describeShortcutTarget(inner)?.isContentEditable).toBe(true);
    expect(suppressed(inner)).toBe(true);
  });

  it("contenteditable=false is not editable", () => {
    const node = el("div", { contenteditable: "false" });

    expect(describeShortcutTarget(node)?.isContentEditable).toBe(false);
    expect(suppressed(node)).toBe(false);
  });

  it("a text widget declared by role is also typing", () => {
    for (const role of ["textbox", "searchbox", "combobox"]) {
      expect(suppressed(el("div", { role })), role).toBe(true);
    }
  });

  it("inside an open dialog belongs to the dialog", () => {
    const shells: Record<string, string>[] = [
      { role: "dialog" },
      { "aria-modal": "true" },
    ];
    for (const attrs of shells) {
      const button = el("div", attrs).appendChild(
        document.createElement("button"),
      );
      expect(describeShortcutTarget(button)?.inDialog, JSON.stringify(attrs)).toBe(
        true,
      );
      expect(suppressed(button), JSON.stringify(attrs)).toBe(true);
    }
  });

  it("a non-element target claims nothing", () => {
    expect(describeShortcutTarget(null)).toBeNull();
    expect(isTextEntryTarget(describeShortcutTarget(new EventTarget()))).toBe(
      false,
    );
  });
});

describe("focusShortcutRegion moves the keyboard to a region", () => {
  it("moves focus to the visible region", () => {
    const list = el("section");
    const attrs = focusRegionAttrs("list");
    list.setAttribute("data-focus-region", attrs["data-focus-region"]);
    list.tabIndex = attrs.tabIndex;

    expect(focusShortcutRegion("list")).toBe(true);
    expect(document.activeElement).toBe(list);
  });

  it("does nothing when the region is missing", () => {
    expect(focusShortcutRegion("content")).toBe(false);
  });

  it("can take the keyboard from the terminal panel", () => {
    // The whole point of the two focus commands: leave the pane for the list.
    const panel = el("div", { "data-terminal-panel": "" });
    const xterm = panel.appendChild(document.createElement("textarea"));
    const list = el("section", { "data-focus-region": "list", tabindex: "-1" });
    xterm.focus();

    expect(terminalPanelHasFocus()).toBe(true);
    expect(focusShortcutRegion("list")).toBe(true);
    expect(document.activeElement).toBe(list);
    expect(terminalPanelHasFocus()).toBe(false);
  });

  it("does not take the keyboard from a caret outside the panel", () => {
    // A resize or a layout signal must never cost somebody the caret they are
    // typing in — the same rule `usePaneFocus` applies.
    const field = el("input", { type: "text" });
    el("section", { "data-focus-region": "list", tabindex: "-1" });
    field.focus();

    expect(focusShortcutRegion("list")).toBe(false);
    expect(document.activeElement).toBe(field);
  });
});

describe("keyboardRegion containing the keyboard", () => {
  it("returns the region containing focus", () => {
    const list = el("div", { "data-focus-region": "list", tabindex: "-1" });
    const inner = list.appendChild(document.createElement("button"));
    inner.focus();

    expect(keyboardRegion()).toBe("list");
  });

  it("returns terminal when focus is inside the terminal panel", () => {
    const panel = el("div", { "data-terminal-panel": "" });
    const xterm = panel.appendChild(document.createElement("textarea"));
    xterm.focus();

    expect(keyboardRegion()).toBe("terminal");
  });

  it("returns null when focus is in no region", () => {
    el("button").focus();
    expect(keyboardRegion()).toBeNull();
  });

  it("returns null for an unknown region name", () => {
    // A marker the ring does not know is not a spot on it.
    const odd = el("div", { "data-focus-region": "footer", tabindex: "-1" });
    odd.focus();
    expect(keyboardRegion()).toBeNull();
  });
});
