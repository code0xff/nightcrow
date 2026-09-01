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
    // leader 시퀀스에는 aria 속성이 없다. 시트의 <kbd>와 버튼이 그 역할을 한다.
    expect(row("terminal.newPane").hasAttribute("aria-keyshortcuts")).toBe(false);
    // standalone chord는 리더와 무관하므로 그대로다.
    expect(keys("project.next")).toEqual(["Ctrl+Shift+ArrowRight"]);
    expect(row("project.next").getAttribute("aria-keyshortcuts")).toBe(
      "Control+Shift+ArrowRight",
    );
  });

  it("코드가_아닌_문자열은_거부를_보여주고_설정을_바꾸지_않는다", () => {
    // `setLeader`가 false를 주는 경우다. 조용히 무시하면 다음 키를 누를 때까지
    // 성공한 재바인딩과 구별되지 않는다.
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
    // 리더 없이도 standalone chord는 살아 있다.
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
    // Ctrl+F는 브라우저의 Find다. 말없이 가져가지 않는 것이 요구사항이다.
    const { settings } = mount({});

    expect(screen.getByRole("status").textContent).toContain("in-page Find");

    type("Alt+G");
    press("Rebind");

    expect(settings.current?.conflict).toBeNull();
    expect(screen.queryByRole("status")).toBeNull();
  });
});
