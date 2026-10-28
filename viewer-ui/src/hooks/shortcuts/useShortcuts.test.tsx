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

describe("useShortcuts leader sequences", () => {
  it("one character after the leader runs one command", () => {
    const { actions } = mount();

    leader();
    press(document.body, { key: "t" });

    expect(actions["terminal.newPane"]).toHaveBeenCalledTimes(1);
  });

  it("a command runs once inside the terminal panel and sends no bytes to the PTY", () => {
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

  it("an unclaimed key passes through untouched to xterm", () => {
    const { xterm, xtermKeydown } = pane();
    mount();

    const event = press(xterm, { key: "a" });

    expect(xtermKeydown).toHaveBeenCalledTimes(1);
    expect(event.defaultPrevented).toBe(false);
  });

  it("holding a key still runs the command only once per press", () => {
    const { actions } = mount();

    leader();
    // The follow-up's autorepeat, then the next physical press.
    const repeated = press(document.body, { key: "t", repeat: true });
    press(document.body, { key: "t" });

    expect(repeated.defaultPrevented).toBe(false);
    expect(actions["terminal.newPane"]).toHaveBeenCalledTimes(1);
  });

  it("auto-repeat for a held code is not a command", () => {
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

  it("all modifiers must match exactly for the leader to be recognized", () => {
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

  it("pressing the leader twice sends one literal leader to the panel", () => {
    const { extras } = mount();

    leader();
    const second = leader();

    // Ctrl+F is the character's place in the ASCII table with the top bits
    // cleared, the same byte `termKeys.ts` sends for a Ctrl chord.
    expect(extras.sendInput).toHaveBeenCalledTimes(1);
    expect(extras.sendInput).toHaveBeenCalledWith("\x06");
    expect(second.defaultPrevented).toBe(true);
  });

  it("an unmapped follow-up key is consumed without running a command", () => {
    const { actions } = mount();

    leader();
    const event = press(document.body, { key: "j" });

    expect(event.defaultPrevented).toBe(true);
    for (const spy of Object.values(actions)) expect(spy).not.toHaveBeenCalled();
  });

  it("Escape and Ctrl+C cancel the leader and the next key works normally", () => {
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

  it("swap accepts pane numbers in two steps", () => {
    const { actions, extras } = mount();

    leader();
    press(document.body, { key: "s" });
    const digit = press(document.body, { key: "3" });

    expect(extras.swapPanes).toHaveBeenCalledWith(1);
    // The digit picked a pane to swap with; it must not also focus it.
    expect(actions["focus.pane1"]).not.toHaveBeenCalled();
    expect(digit.defaultPrevented).toBe(true);
  });

  it("a non-pane key in the second swap step cancels the swap", () => {
    const { actions, extras } = mount();

    leader();
    press(document.body, { key: "s" });
    press(document.body, { key: "t" });

    expect(extras.swapPanes).not.toHaveBeenCalled();
    // Nor does it run the command the key names: the person asked to swap.
    expect(actions["terminal.newPane"]).not.toHaveBeenCalled();
  });

  it("a registered code without a leader runs immediately", () => {
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

  it("when the leader is disabled, its code passes through", () => {
    const { actions } = mount({ leader: null });

    const event = leader();
    press(document.body, { key: "t" });

    expect(event.defaultPrevented).toBe(false);
    expect(actions["terminal.newPane"]).not.toHaveBeenCalled();
  });

  it("changing the leader recognizes only the new code", () => {
    const { actions, update } = mount();

    update({ leader: { key: "B", ctrl: false, shift: false, alt: true, meta: false } });
    press(document.body, { key: "b", altKey: true });
    press(document.body, { key: "t" });

    expect(actions["terminal.newPane"]).toHaveBeenCalledTimes(1);
  });

  it("intercepts no keys when enabled is false", () => {
    const { actions } = mount({ enabled: false });

    const event = leader();
    press(document.body, { key: "t" });

    expect(event.defaultPrevented).toBe(false);
    expect(actions["terminal.newPane"]).not.toHaveBeenCalled();
  });
});
