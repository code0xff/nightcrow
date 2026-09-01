// @vitest-environment happy-dom
//
// The promise this file exists to keep: a chord the page claims runs its command
// exactly once and puts nothing on the wire, and a chord it does not claim is
// left completely alone so xterm encodes it and the PTY receives what was typed.

import { cleanup } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import { leader, mount, pane, press } from "./useShortcuts.harness";

afterEach(() => {
  cleanup();
  document.body.innerHTML = "";
});

describe("useShortcuts 리더 시퀀스", () => {
  it("리더_다음_한_글자는_명령을_한_번_실행한다", () => {
    const { actions } = mount();

    leader();
    press(document.body, { key: "t" });

    expect(actions["terminal.newPane"]).toHaveBeenCalledTimes(1);
  });

  it("터미널_패널_안에서_명령은_한_번_돌고_PTY로_한_바이트도_가지_않는다", () => {
    const { xterm, xtermKeydown } = pane();
    const { actions, extras } = mount();

    const armed = leader(xterm);
    const run = press(xterm, { key: "t" });

    expect(xtermKeydown).not.toHaveBeenCalled();
    expect(extras.sendInput).not.toHaveBeenCalled();
    expect(actions["terminal.newPane"]).toHaveBeenCalledTimes(1);
    expect(armed.defaultPrevented).toBe(true);
    expect(run.defaultPrevented).toBe(true);
  });

  it("청구하지_않은_키는_xterm까지_손대지_않고_그대로_간다", () => {
    const { xterm, xtermKeydown } = pane();
    mount();

    const event = press(xterm, { key: "a" });

    expect(xtermKeydown).toHaveBeenCalledTimes(1);
    expect(event.defaultPrevented).toBe(false);
  });

  it("키를_누르고_있어도_한_번_누를_때_한_번만_실행한다", () => {
    const { actions } = mount();

    leader();
    // The follow-up's autorepeat, then the next physical press.
    const repeated = press(document.body, { key: "t", repeat: true });
    press(document.body, { key: "t" });

    expect(repeated.defaultPrevented).toBe(false);
    expect(actions["terminal.newPane"]).toHaveBeenCalledTimes(1);
  });

  it("눌러둔_코드의_자동반복은_명령이_아니다", () => {
    const { actions } = mount();

    const event = press(document.body, {
      key: "ArrowRight",
      ctrlKey: true,
      shiftKey: true,
      repeat: true,
    });

    expect(event.defaultPrevented).toBe(false);
    expect(actions["project.next"]).not.toHaveBeenCalled();
  });

  it("모든_수식키가_정확히_맞아야_리더로_인정한다", () => {
    for (const extra of [
      { shiftKey: true },
      { altKey: true },
      { metaKey: true },
      { ctrlKey: false },
    ]) {
      const { actions } = mount();
      const event = leader(document.body, extra);
      press(document.body, { key: "t" });
      expect(event.defaultPrevented, JSON.stringify(extra)).toBe(false);
      expect(actions["terminal.newPane"]).not.toHaveBeenCalled();
      cleanup();
    }
  });

  it("리더를_두_번_누르면_리터럴_리더_한_번을_패널로_보낸다", () => {
    const { extras } = mount();

    leader();
    const second = leader();

    // Ctrl+F is the character's place in the ASCII table with the top bits
    // cleared, the same byte `termKeys.ts` sends for a Ctrl chord.
    expect(extras.sendInput).toHaveBeenCalledTimes(1);
    expect(extras.sendInput).toHaveBeenCalledWith("\x06");
    expect(second.defaultPrevented).toBe(true);
  });

  it("매핑되지_않은_따라오는_키는_먹히고_아무_명령도_돌지_않는다", () => {
    const { actions } = mount();

    leader();
    const event = press(document.body, { key: "j" });

    expect(event.defaultPrevented).toBe(true);
    for (const spy of Object.values(actions)) expect(spy).not.toHaveBeenCalled();
  });

  it("Escape와_Ctrl_C는_리더를_취소하고_다음_키는_평소처럼_동작한다", () => {
    for (const init of [{ key: "Escape" }, { key: "c", ctrlKey: true }]) {
      const { actions } = mount();

      leader();
      const cancelled = press(document.body, init);
      // Idle again: `t` is an ordinary key, not the new-pane command.
      const after = press(document.body, { key: "t" });

      expect(cancelled.defaultPrevented, init.key).toBe(true);
      expect(after.defaultPrevented, init.key).toBe(false);
      expect(actions["terminal.newPane"]).not.toHaveBeenCalled();
      cleanup();
    }
  });

  it("스왑은_두_단계로_pane_숫자를_받는다", () => {
    const { actions, extras } = mount();

    leader();
    press(document.body, { key: "s" });
    const digit = press(document.body, { key: "3" });

    expect(extras.swapPanes).toHaveBeenCalledWith(1);
    // The digit picked a pane to swap with; it must not also focus it.
    expect(actions["focus.pane1"]).not.toHaveBeenCalled();
    expect(digit.defaultPrevented).toBe(true);
  });

  it("스왑_두번째_단계의_pane이_아닌_키는_스왑을_버린다", () => {
    const { actions, extras } = mount();

    leader();
    press(document.body, { key: "s" });
    press(document.body, { key: "t" });

    expect(extras.swapPanes).not.toHaveBeenCalled();
    // Nor does it run the command the key names: the person asked to swap.
    expect(actions["terminal.newPane"]).not.toHaveBeenCalled();
  });

  it("리더_없는_등록된_코드는_바로_실행된다", () => {
    const { actions } = mount();

    const right = press(document.body, {
      key: "ArrowRight",
      ctrlKey: true,
      shiftKey: true,
    });
    press(document.body, { key: "ArrowLeft", ctrlKey: true, shiftKey: true });

    expect(actions["project.next"]).toHaveBeenCalledTimes(1);
    expect(actions["project.previous"]).toHaveBeenCalledTimes(1);
    expect(right.defaultPrevented).toBe(true);
  });

  it("리더를_끄면_리더_코드는_그대로_통과한다", () => {
    const { actions } = mount({ leader: null });

    const event = leader();
    press(document.body, { key: "t" });

    expect(event.defaultPrevented).toBe(false);
    expect(actions["terminal.newPane"]).not.toHaveBeenCalled();
  });

  it("리더를_바꾸면_새_코드만_인정한다", () => {
    const { actions, update } = mount();

    update({ leader: { key: "B", ctrl: false, shift: false, alt: true, meta: false } });
    press(document.body, { key: "b", altKey: true });
    press(document.body, { key: "t" });

    expect(actions["terminal.newPane"]).toHaveBeenCalledTimes(1);
  });

  it("enabled가_false면_아무_키도_가로채지_않는다", () => {
    const { actions } = mount({ enabled: false });

    const event = leader();
    press(document.body, { key: "t" });

    expect(event.defaultPrevented).toBe(false);
    expect(actions["terminal.newPane"]).not.toHaveBeenCalled();
  });
});
