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

describe("describeShortcutTarget 키가 떨어진 곳", () => {
  it("터미널_패널_안은_타이핑이_아니다", () => {
    // xterm's input surface is a `<textarea>`, which the text-field rule matches
    // by tag alone. Inside the panel the page owns the keyboard.
    const panel = el("div", { "data-terminal-panel": "" });
    const xterm = panel.appendChild(document.createElement("textarea"));

    expect(describeShortcutTarget(xterm)).toBeNull();
    expect(suppressed(xterm)).toBe(false);
  });

  it("패널_밖의_textarea는_타이핑이다", () => {
    expect(suppressed(el("textarea"))).toBe(true);
  });

  it("입력_타입을_그대로_전달한다", () => {
    expect(describeShortcutTarget(el("input", { type: "CHECKBOX" }))?.type).toBe(
      "checkbox",
    );
    expect(suppressed(el("input", { type: "checkbox" }))).toBe(false);
    expect(suppressed(el("input", { type: "password" }))).toBe(true);
  });

  it("편집_가능한_조상을_읽는다", () => {
    const host = el("div", { contenteditable: "true" });
    const inner = host.appendChild(document.createElement("span"));

    expect(describeShortcutTarget(inner)?.isContentEditable).toBe(true);
    expect(suppressed(inner)).toBe(true);
  });

  it("contenteditable_false는_편집이_아니다", () => {
    const node = el("div", { contenteditable: "false" });

    expect(describeShortcutTarget(node)?.isContentEditable).toBe(false);
    expect(suppressed(node)).toBe(false);
  });

  it("역할로_선언한_텍스트_위젯도_타이핑이다", () => {
    for (const role of ["textbox", "searchbox", "combobox"]) {
      expect(suppressed(el("div", { role })), role).toBe(true);
    }
  });

  it("열린_다이얼로그_안은_다이얼로그의_것이다", () => {
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

  it("엘리먼트가_아닌_대상은_아무것도_주장하지_않는다", () => {
    expect(describeShortcutTarget(null)).toBeNull();
    expect(isTextEntryTarget(describeShortcutTarget(new EventTarget()))).toBe(
      false,
    );
  });
});

describe("focusShortcutRegion 영역으로 키보드 옮기기", () => {
  it("표시된_영역으로_포커스를_옮긴다", () => {
    const list = el("section");
    const attrs = focusRegionAttrs("list");
    list.setAttribute("data-focus-region", attrs["data-focus-region"]);
    list.tabIndex = attrs.tabIndex;

    expect(focusShortcutRegion("list")).toBe(true);
    expect(document.activeElement).toBe(list);
  });

  it("영역이_없으면_아무것도_하지_않는다", () => {
    expect(focusShortcutRegion("content")).toBe(false);
  });

  it("터미널_패널에서는_키보드를_가져올_수_있다", () => {
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

  it("패널_밖의_캐럿에서는_키보드를_가져오지_않는다", () => {
    // A resize or a layout signal must never cost somebody the caret they are
    // typing in — the same rule `usePaneFocus` applies.
    const field = el("input", { type: "text" });
    el("section", { "data-focus-region": "list", tabindex: "-1" });
    field.focus();

    expect(focusShortcutRegion("list")).toBe(false);
    expect(document.activeElement).toBe(field);
  });
});

describe("keyboardRegion 키보드가 있는 영역", () => {
  it("포커스_영역_안이면_그_영역이다", () => {
    const list = el("div", { "data-focus-region": "list", tabindex: "-1" });
    const inner = list.appendChild(document.createElement("button"));
    inner.focus();

    expect(keyboardRegion()).toBe("list");
  });

  it("터미널_패널_안이면_terminal이다", () => {
    const panel = el("div", { "data-terminal-panel": "" });
    const xterm = panel.appendChild(document.createElement("textarea"));
    xterm.focus();

    expect(keyboardRegion()).toBe("terminal");
  });

  it("어느_영역에도_없으면_null이다", () => {
    el("button").focus();
    expect(keyboardRegion()).toBeNull();
  });

  it("알_수_없는_영역_이름은_null이다", () => {
    // A marker the ring does not know is not a spot on it.
    const odd = el("div", { "data-focus-region": "footer", tabindex: "-1" });
    odd.focus();
    expect(keyboardRegion()).toBeNull();
  });
});
