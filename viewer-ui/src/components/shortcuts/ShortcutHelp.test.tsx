// @vitest-environment happy-dom

import { cleanup, fireEvent, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { mount, row, sheet } from "./ShortcutHelp.harness";
import {
  SHORTCUT_ACTIONS,
  UNSUPPORTED_TUI_ACTIONS,
} from "../../lib/shortcuts/shortcutActions";

afterEach(cleanup);

describe("ShortcutHelp", () => {
  it("renders one row for every registered action", () => {
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

  it("marks reinterpreted actions as such", () => {
    mount({});

    const maximize = row("view.toggleMaximize");
    expect(maximize.textContent).toContain("reinterpreted");
    expect(maximize.textContent).toContain("never bound to F11");
  });

  it("shows TUI keys that are not bound in the browser with the reason", () => {
    mount({});

    for (const entry of UNSUPPORTED_TUI_ACTIONS) {
      expect(document.body.textContent).toContain(entry.label);
      expect(document.body.textContent).toContain(entry.reason);
    }
  });

  it("shows unavailable rows and makes clicking them a no-op", () => {
    mount({ "focus.list": vi.fn() });

    const unavailable = row("terminal.newPane");
    expect(unavailable.getAttribute("aria-disabled")).toBe("true");
    fireEvent.click(unavailable);

    expect(sheet()).not.toBeNull();
  });

  it("runs an available action and closes the sheet", () => {
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

  it("a row that arms the second step is not a button and does nothing when clicked", () => {
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

  it("all other rows are clickable buttons", () => {
    mount({});

    for (const action of SHORTCUT_ACTIONS) {
      if (action.keyboardOnly) continue;
      expect(row(action.id).tagName, action.id).toBe("BUTTON");
    }
  });

  it("an arming row also reports unavailable when it has no handler", () => {
    // Availability keeps its single source: the row is text either way, but it
    // still dims when there is no panel with a pane to swap.
    mount({});

    expect(
      row("terminal.swapPanePrompt").getAttribute("aria-disabled"),
    ).toBe("true");
  });

  it("no sheet row has an empty aria-keyshortcuts attribute", () => {
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

  it("opens as a named modal", () => {
    mount({});

    const dialog = screen.getByRole("dialog");
    expect(dialog.getAttribute("aria-modal")).toBe("true");
    // `shortcutDom` also uses these attributes to identify the dialog.
    expect(dialog).toHaveProperty("tabIndex", -1);
    expect(screen.getByRole("dialog", { name: "Keyboard shortcuts" })).toBe(
      dialog,
    );
  });

  it("the sheet receives keyboard focus when opened", () => {
    mount({});

    expect(document.activeElement).toBe(screen.getByRole("dialog"));
  });

  it("closes with Escape and returns focus to the opening button", () => {
    const { opener } = mount({});

    fireEvent.keyDown(document, { key: "Escape" });

    expect(sheet()).toBeNull();
    expect(document.activeElement).toBe(opener);
  });

  it("clicking the backdrop closes it", () => {
    const { opener } = mount({});

    // As with `FolderPicker`, clicks inside the sheet must not bubble out.
    fireEvent.click(screen.getByRole("dialog"));
    expect(sheet()).not.toBeNull();

    fireEvent.click(screen.getByRole("dialog").parentElement!);
    expect(sheet()).toBeNull();
    expect(document.activeElement).toBe(opener);
  });
});
