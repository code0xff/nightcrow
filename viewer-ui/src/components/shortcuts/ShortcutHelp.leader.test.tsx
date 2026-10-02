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
  it("기본_리더가_각_줄의_키로_찍힌다", () => {
    mount({});

    expect(keys("terminal.newPane")).toEqual(["Ctrl+F", "t"]);
    expect(keys("project.next")).toEqual(["Ctrl+Shift+ArrowRight"]);
  });

  it("리더를_바꾸면_찍힌_키와_aria_값이_같이_바뀐다", () => {
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

  it("코드가_아닌_문자열은_거부를_보여주고_설정을_바꾸지_않는다", () => {
    // A rejected binding must be visible instead of looking successful until
    // the next keystroke.
    const { settings } = mount({});

    type("Ctrl+");
    press("Rebind");

    expect(screen.getByRole("alert").textContent).toContain("is not a chord");
    expect(settings.current?.leaderText).toBe("Ctrl+F");
    expect(keys("terminal.newPane")).toEqual(["Ctrl+F", "t"]);
  });

  it("리더를_끄면_leader_액션에_키가_없다고_말한다", () => {
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

  it("Reset은_기본_리더를_되돌린다", () => {
    const { settings } = mount({});

    press("Switch off");
    press("Reset");

    expect(settings.current?.leaderText).toBe("Ctrl+F");
    expect(keys("terminal.newPane")).toEqual(["Ctrl+F", "t"]);
  });

  it("기본_리더의_브라우저_충돌을_눈에_보이게_경고한다", () => {
    // Ctrl+F is the browser's Find, so taking it must be reported.
    const { settings } = mount({});

    expect(screen.getByRole("status").textContent).toContain("in-page Find");

    type("Alt+G");
    press("Rebind");

    expect(settings.current?.conflict).toBeNull();
    expect(screen.queryByRole("status")).toBeNull();
  });
});
