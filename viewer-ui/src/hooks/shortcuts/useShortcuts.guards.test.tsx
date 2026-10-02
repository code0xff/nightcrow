// @vitest-environment happy-dom
//
// Where the keyboard is not the page's. A shortcut that fires while someone is
// filling in the file filter eats a character and they cannot see why, and
// `Ctrl+Shift+Arrow` in a text field is the OS's extend-selection-by-word
// gesture — taking that away is a regression nobody can work around.

import { cleanup } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import {
  el,
  leader,
  mount,
  mountBare,
  mounted,
  press,
  type Init,
} from "./useShortcuts.harness";

afterEach(() => {
  cleanup();
  document.body.innerHTML = "";
});

const CHORD: Init = { key: "ArrowRight", ctrlKey: true, shiftKey: true };

/** Every shape of text entry the suppression rule recognises. */
function textEntries(): HTMLElement[] {
  return [
    el("input", { type: "text" }),
    el("input", { type: "password" }),
    el("textarea"),
    el("select"),
    el("div", { contenteditable: "true" }),
    ...["textbox", "searchbox", "combobox"].map((role) => el("div", { role })),
  ];
}

describe("useShortcuts 키보드를 넘겨주는 경우", () => {
  it("텍스트_입력_안에서는_단어_선택_제스처를_건드리지_않는다", () => {
    for (const node of textEntries()) {
      const { actions } = mount();

      const event = press(mounted(node), CHORD);

      expect(actions["project.next"], node.outerHTML).not.toHaveBeenCalled();
      expect(event.defaultPrevented, node.outerHTML).toBe(false);
      cleanup();
      document.body.innerHTML = "";
    }
  });

  it("텍스트_입력_안에서는_리더도_가로채지_않는다", () => {
    const field = mounted(el("input", { type: "text" }));
    const { actions } = mount();

    const armed = leader(field);
    const follow = press(field, { key: "t" });

    expect(armed.defaultPrevented).toBe(false);
    expect(follow.defaultPrevented).toBe(false);
    expect(actions["terminal.newPane"]).not.toHaveBeenCalled();
  });

  it("텍스트_입력의_자손에서_올라온_키도_입력의_것이다", () => {
    // A `contenteditable` reports its keystrokes against whatever node the caret
    // is in, which is why the rule looks at ancestors and not only the target.
    const host = mounted(el("div", { contenteditable: "true" }));
    const inner = host.appendChild(el("span"));
    const { actions } = mount();

    const event = press(inner, CHORD);

    expect(actions["project.next"]).not.toHaveBeenCalled();
    expect(event.defaultPrevented).toBe(false);
  });

  it("contenteditable_false는_텍스트_입력이_아니다", () => {
    const node = mounted(el("div", { contenteditable: "false" }));
    const { actions } = mount();

    press(node, CHORD);

    expect(actions["project.next"]).toHaveBeenCalledTimes(1);
  });

  it("글자를_받지_않는_입력_타입에서는_명령이_돈다", () => {
    // A space or a letter on a checkbox is the control's own gesture, not
    // typing, so the page may claim the key.
    const node = mounted(el("input", { type: "checkbox" }));
    const { actions } = mount();

    press(node, CHORD);

    expect(actions["project.next"]).toHaveBeenCalledTimes(1);
  });

  it("열린_다이얼로그_안에서는_키를_다이얼로그에_맡긴다", () => {
    const shells = [
      el("div", { role: "dialog" }),
      el("div", { "aria-modal": "true" }),
      el("dialog", { open: "" }),
    ];
    for (const shell of shells) {
      const button = mounted(shell).appendChild(el("button"));
      const { actions } = mount();

      const event = press(button, CHORD);

      expect(actions["project.next"], shell.outerHTML).not.toHaveBeenCalled();
      expect(event.defaultPrevented, shell.outerHTML).toBe(false);
      cleanup();
      document.body.innerHTML = "";
    }
  });

  it("모달이_열려_있다고_들으면_아무_키도_가로채지_않는다", () => {
    // The folder picker is a plain overlay, so the page says so rather than
    // leaving it to be recognised from the DOM.
    const { actions } = mount({ dialogOpen: true });

    const chord = press(document.body, CHORD);
    const armed = leader();

    expect(actions["project.next"]).not.toHaveBeenCalled();
    expect(chord.defaultPrevented).toBe(false);
    expect(armed.defaultPrevented).toBe(false);
  });

  it("조합_중인_입력은_명령이_아니다", () => {
    const cases: Init[] = [
      { ...CHORD, isComposing: true },
      { ...CHORD, keyCode: 229 },
      { ...CHORD, key: "Process" },
      { ...CHORD, key: "Unidentified" },
    ];
    for (const init of cases) {
      const { actions } = mount();

      const event = press(document.body, init);

      const label = JSON.stringify(init);
      expect(actions["project.next"], label).not.toHaveBeenCalled();
      expect(event.defaultPrevented, label).toBe(false);
      cleanup();
    }
  });

  it("조합_중인_키는_리더의_따라오는_키로도_읽히지_않는다", () => {
    const { actions } = mount();

    leader();
    const composing = press(document.body, { key: "t", isComposing: true });

    expect(composing.defaultPrevented).toBe(false);
    expect(actions["terminal.newPane"]).not.toHaveBeenCalled();
  });

  it("핸들러가_없는_명령의_키도_리더가_먹는다", () => {
    // Nothing is registered here at all. The leader still owns its follow-up,
    // or `<prefix> t` would type a `t` into the shell of a page whose terminal
    // panel has not finished mounting.
    mountBare();

    leader();
    const event = press(document.body, { key: "t" });

    expect(event.defaultPrevented).toBe(true);
  });
});
