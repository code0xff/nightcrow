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
    mount({ "focus.list": vi.fn() });

    const unavailable = row("terminal.newPane");
    expect(unavailable.getAttribute("aria-disabled")).toBe("true");
    fireEvent.click(unavailable);

    expect(sheet()).not.toBeNull();
  });

  it("실행할_수_있는_줄은_그_액션을_돌리고_시트를_닫는다", () => {
    // These focus actions have no other button, so the row is their only
    // non-keyboard path.
    const focusList = vi.fn();
    const newPane = vi.fn();
    mount({ "focus.list": focusList, "terminal.newPane": newPane });

    fireEvent.click(row("focus.list"));

    expect(focusList).toHaveBeenCalledTimes(1);
    expect(newPane).not.toHaveBeenCalled();
    expect(sheet()).toBeNull();
  });

  it("두_번째_단계를_무장하는_줄은_버튼이_아니고_눌러도_아무_일이_없다", () => {
    // `<prefix> s` waits for another key, so a click has no action; render text
    // instead of a button that appears broken.
    const swap = vi.fn();
    mount({ "terminal.swapPanePrompt": swap });

    const arming = row("terminal.swapPanePrompt");
    expect(arming.tagName).not.toBe("BUTTON");
    fireEvent.click(arming);

    expect(swap).not.toHaveBeenCalled();
    expect(sheet()).not.toBeNull();
    // Not a button, but still advertised as runnable by the keyboard.
    expect(arming.hasAttribute("aria-disabled")).toBe(false);
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
    // The registry supplies every row: leader sequences have no ARIA value,
    // while standalone chords do.
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
    // `shortcutDom` also uses these attributes to identify the dialog.
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

    // As with `FolderPicker`, clicks inside the sheet must not bubble out.
    fireEvent.click(screen.getByRole("dialog"));
    expect(sheet()).not.toBeNull();

    fireEvent.click(screen.getByRole("dialog").parentElement!);
    expect(sheet()).toBeNull();
    expect(document.activeElement).toBe(opener);
  });
});
