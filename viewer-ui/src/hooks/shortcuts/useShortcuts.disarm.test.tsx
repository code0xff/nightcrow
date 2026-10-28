// @vitest-environment happy-dom
//
// The leader waits indefinitely for a follow-up, so a leader left armed is a key
// the person types into their shell and never sees. Every signal that ends the
// moment it was pressed in has to put it back to idle — and after each one the
// next key must behave exactly as it would have if the leader had never been
// pressed, which is what the second half of each case here asserts.

import { cleanup } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import { el, leader, mount, mounted, pane, press } from "./useShortcuts.harness";

afterEach(() => {
  cleanup();
  document.body.innerHTML = "";
});

/** After a disarm, `t` is an ordinary key again: nothing runs and nothing is
 *  taken from the pane. */
function expectIdle(actions: Record<string, { mock: { calls: unknown[] } }>) {
  const after = press(document.body, { key: "t" });
  expect(after.defaultPrevented).toBe(false);
  expect(actions["terminal.newPane"].mock.calls).toHaveLength(0);
}

describe("useShortcuts leader disarming", () => {
  it("disarms when the window loses focus", () => {
    const { actions } = mount();

    leader();
    window.dispatchEvent(new Event("blur"));

    expectIdle(actions);
  });

  it("disarms when focus moves to a text input", () => {
    const field = mounted(el("input", { type: "text" }));
    const { actions } = mount();

    leader();
    field.dispatchEvent(new FocusEvent("focusin", { bubbles: true }));

    expectIdle(actions);
  });

  it("disarms when focus moves into a dialog", () => {
    const button = mounted(el("div", { role: "dialog" })).appendChild(
      el("button"),
    );
    const { actions } = mount();

    leader();
    button.dispatchEvent(new FocusEvent("focusin", { bubbles: true }));

    expectIdle(actions);
  });

  it("moving focus to the terminal panel does not disarm", () => {
    // Arming the leader and then clicking the pane it is meant for must not
    // throw the leader away.
    const { xterm } = pane();
    const { actions } = mount();

    leader();
    xterm.dispatchEvent(new FocusEvent("focusin", { bubbles: true }));
    press(xterm, { key: "t" });

    expect(actions["terminal.newPane"]).toHaveBeenCalledTimes(1);
  });

  it("disarms when a modal opens", () => {
    const { actions, update } = mount();

    leader();
    update({ dialogOpen: true });
    update({ dialogOpen: false });

    expectIdle(actions);
  });

  it("disarms when switching projects", () => {
    // The terminal socket is keyed on the repository, so a switch also tears
    // every pane down and hands out ids that mean something else.
    const { actions, update } = mount();

    leader();
    update({ repo: "r2" });

    expectIdle(actions);
  });

  it("disarms when notified of a socket reconnect", () => {
    const { actions, bus } = mount();

    leader();
    bus.current?.disarm();

    expectIdle(actions);
  });

  it("disarms when a key is pressed in a text input", () => {
    // Suppression is also a disarm: the classifier is pure and cannot do it, so
    // the engine feeds the reducer a `suppressed` event of its own.
    const field = mounted(el("input", { type: "text" }));
    const { actions } = mount();

    leader();
    press(field, { key: "x" });

    expectIdle(actions);
  });

  it("disarms when the keyboard is disabled", () => {
    // The session expiring puts the login screen up and takes the listener away
    // with `enabled`. Coming back has to come back idle: otherwise the first key
    // typed into a pane after signing back in is spent on the leader nobody
    // remembers pressing.
    const { actions, update } = mount();

    leader();
    update({ enabled: false });
    update({ enabled: true });

    expectIdle(actions);
  });

  it("the leader can be armed again after disarming", () => {
    const { actions } = mount();

    leader();
    window.dispatchEvent(new Event("blur"));
    leader();
    press(document.body, { key: "t" });

    expect(actions["terminal.newPane"]).toHaveBeenCalledTimes(1);
  });
});
