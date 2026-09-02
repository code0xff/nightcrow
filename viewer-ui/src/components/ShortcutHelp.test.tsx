// @vitest-environment happy-dom

import { cleanup, fireEvent, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { mount, row, sheet } from "./ShortcutHelp.harness";
import {
  SHORTCUT_ACTIONS,
  UNSUPPORTED_TUI_ACTIONS,
} from "../lib/shortcutActions";

afterEach(cleanup);

describe("ShortcutHelp", () => {
  it("레지스트리의_모든_액션이_한_줄씩_나온다", () => {
    // 레지스트리에 액션을 더하면 아무 것도 기억하지 않아도 여기에 나타나야
    // 한다. 시트가 자기 키 표를 들고 있지 않다는 것이 이 단정이다.
    mount({});

    const listed = [...document.querySelectorAll("[data-shortcut-action]")].map(
      (node) => node.getAttribute("data-shortcut-action"),
    );
    expect(new Set(listed)).toEqual(new Set(SHORTCUT_ACTIONS.map((a) => a.id)));
    expect(listed).toHaveLength(SHORTCUT_ACTIONS.length);
    for (const action of SHORTCUT_ACTIONS) {
      expect(row(action.id).textContent).toContain(action.label);
    }
  });

  it("reinterpreted_액션은_그렇다고_표시된다", () => {
    mount({});

    const maximize = row("view.toggleMaximize");
    expect(maximize.textContent).toContain("reinterpreted");
    // 등록된 note가 그대로 보여야 TUI와 무엇이 다른지 읽을 수 있다.
    expect(maximize.textContent).toContain("never bound to F11");
  });

  it("브라우저에서_묶지_않는_TUI_키는_이유와_함께_나온다", () => {
    mount({});

    for (const entry of UNSUPPORTED_TUI_ACTIONS) {
      expect(document.body.textContent).toContain(entry.label);
      expect(document.body.textContent).toContain(entry.reason);
    }
  });

  it("실행할_수_없는_줄은_표시되고_눌러도_아무_일이_없다", () => {
    // 핸들러가 없는 액션은 지금 이 화면에서 할 수 없는 일이다.
    mount({ "focus.list": vi.fn() });

    const unavailable = row("terminal.newPane");
    expect(unavailable.getAttribute("aria-disabled")).toBe("true");
    fireEvent.click(unavailable);

    expect(sheet()).not.toBeNull();
  });

  it("실행할_수_있는_줄은_그_액션을_돌리고_시트를_닫는다", () => {
    // `focus.list`와 `focus.content`는 다른 어떤 버튼도 없어서, 이 줄이
    // 키보드가 아닌 유일한 경로다.
    const focusList = vi.fn();
    const newPane = vi.fn();
    mount({ "focus.list": focusList, "terminal.newPane": newPane });

    fireEvent.click(row("focus.list"));

    expect(focusList).toHaveBeenCalledTimes(1);
    expect(newPane).not.toHaveBeenCalled();
    expect(sheet()).toBeNull();
  });

  it("두_번째_단계를_무장하는_줄은_버튼이_아니고_눌러도_아무_일이_없다", () => {
    // `<prefix> s`는 다음 키를 기다리므로 클릭이 줄 수 있는 것이 없다. 아무
    // 일도 하지 않는 버튼은 고장으로 읽히므로 텍스트로 렌더한다.
    const swap = vi.fn();
    mount({ "terminal.swapPanePrompt": swap });

    const arming = row("terminal.swapPanePrompt");
    expect(arming.tagName).not.toBe("BUTTON");
    fireEvent.click(arming);

    expect(swap).not.toHaveBeenCalled();
    expect(sheet()).not.toBeNull();
    // Not a button, but still advertised as runnable by the keyboard.
    expect(arming.hasAttribute("aria-disabled")).toBe(false);
    // 이유는 여전히 그 줄에 적혀 있다.
    expect(arming.textContent).toContain("the next pane digit");
  });

  it("그_밖의_모든_줄은_누를_수_있는_버튼이다", () => {
    mount({});

    for (const action of SHORTCUT_ACTIONS) {
      if (action.keyboardOnly) continue;
      expect(row(action.id).tagName, action.id).toBe("BUTTON");
    }
  });

  it("무장하는_줄도_핸들러가_없으면_가용하지_않다고_말한다", () => {
    // Availability keeps its single source: the row is text either way, but it
    // still dims when there is no panel with a pane to swap.
    mount({});

    expect(
      row("terminal.swapPanePrompt").getAttribute("aria-disabled"),
    ).toBe("true");
  });

  it("시트의_어떤_줄도_빈_aria_keyshortcuts를_갖지_않는다", () => {
    // 시트는 레지스트리 전체를 렌더하므로 여기서 한 번에 확인된다: leader
    // 시퀀스 줄은 속성이 없고, chord 줄만 값을 가진다.
    mount({});

    const marked = [...document.querySelectorAll("[aria-keyshortcuts]")];
    expect(marked).toHaveLength(
      SHORTCUT_ACTIONS.filter((action) => action.chord).length,
    );
    for (const node of marked) {
      const value = node.getAttribute("aria-keyshortcuts");
      expect(value?.trim()).toBeTruthy();
      expect(value).not.toContain("undefined");
    }
  });

  it("모달로_열리고_이름을_가진다", () => {
    mount({});

    const dialog = screen.getByRole("dialog");
    expect(dialog.getAttribute("aria-modal")).toBe("true");
    // 이 두 속성은 `shortcutDom`이 dialog를 알아보는 방법이기도 하다.
    expect(dialog).toHaveProperty("tabIndex", -1);
    expect(screen.getByRole("dialog", { name: "Keyboard shortcuts" })).toBe(
      dialog,
    );
  });

  it("열리면_시트가_키보드를_받는다", () => {
    mount({});

    expect(document.activeElement).toBe(screen.getByRole("dialog"));
  });

  it("Escape로_닫히고_키보드는_열었던_버튼으로_돌아간다", () => {
    const { opener } = mount({});

    fireEvent.keyDown(document, { key: "Escape" });

    expect(sheet()).toBeNull();
    expect(document.activeElement).toBe(opener);
  });

  it("배경을_누르면_닫힌다", () => {
    const { opener } = mount({});

    // `FolderPicker`와 같은 규약: 시트 안의 클릭은 전파되지 않는다.
    fireEvent.click(screen.getByRole("dialog"));
    expect(sheet()).not.toBeNull();

    fireEvent.click(screen.getByRole("dialog").parentElement!);
    expect(sheet()).toBeNull();
    expect(document.activeElement).toBe(opener);
  });
});
