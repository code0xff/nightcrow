// @vitest-environment happy-dom

import { cleanup, fireEvent, screen } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import { mount, row } from "./ShortcutHelp.harness";

afterEach(cleanup);

/** The keys printed on one row, in the order they are pressed. */
function keys(id: string): string[] {
  return [...row(id).querySelectorAll("kbd")].map((kbd) => kbd.textContent ?? "");
}

function type(text: string): void {
  fireEvent.change(screen.getByLabelText("leader chord"), {
    target: { value: text },
  });
}

function press(name: string): void {
  fireEvent.click(screen.getByRole("button", { name }));
}

describe("ShortcutHelp leader configuration", () => {
  it("the default leader is shown as the key on each row", () => {
    mount({});

    expect(keys("terminal.newPane")).toEqual(["Ctrl+F", "t"]);
    expect(keys("project.next")).toEqual(["Ctrl+Shift+ArrowRight"]);
  });

  it("changing the leader updates both the displayed key and aria value", () => {
    const { settings } = mount({});

    type("Alt+G");
    press("Rebind");

    expect(settings.current?.leaderText).toBe("Alt+G");
    expect(keys("terminal.newPane")).toEqual(["Alt+G", "t"]);
    // ARIA has no two-step syntax; the sheet's <kbd> and button expose it.
    expect(row("terminal.newPane").hasAttribute("aria-keyshortcuts")).toBe(false);
    // Standalone chords do not depend on the leader.
    expect(keys("project.next")).toEqual(["Ctrl+Shift+ArrowRight"]);
    expect(row("project.next").getAttribute("aria-keyshortcuts")).toBe(
      "Control+Shift+ArrowRight",
    );
  });

  it("a non-code string shows a rejection and does not change the setting", () => {
    // A rejected binding must be visible instead of looking successful until
    // the next keystroke.
    const { settings } = mount({});

    type("Ctrl+");
    press("Rebind");

    expect(screen.getByRole("alert").textContent).toContain("is not a chord");
    expect(settings.current?.leaderText).toBe("Ctrl+F");
    expect(keys("terminal.newPane")).toEqual(["Ctrl+F", "t"]);
  });

  it("when the leader is disabled, leader actions report that they have no key", () => {
    const { settings } = mount({});

    press("Switch off");

    expect(settings.current?.leader).toBeNull();
    expect(keys("terminal.newPane")).toEqual([]);
    expect(row("terminal.newPane").textContent).toContain(
      "the leader is switched off",
    );
    expect(row("terminal.newPane").hasAttribute("aria-keyshortcuts")).toBe(false);
    expect(keys("project.next")).toEqual(["Ctrl+Shift+ArrowRight"]);
  });

  it("Reset restores the default leader", () => {
    const { settings } = mount({});

    press("Switch off");
    press("Reset");

    expect(settings.current?.leaderText).toBe("Ctrl+F");
    expect(keys("terminal.newPane")).toEqual(["Ctrl+F", "t"]);
  });

  it("visibly warns about browser conflicts with the default leader", () => {
    // Ctrl+F is the browser's Find, so taking it must be reported.
    const { settings } = mount({});

    expect(screen.getByRole("status").textContent).toContain("in-page Find");

    type("Alt+G");
    press("Rebind");

    expect(settings.current?.conflict).toBeNull();
    expect(screen.queryByRole("status")).toBeNull();
  });
});
