// @vitest-environment happy-dom
//
// The first hook-level test, and deliberately the one that leans on what the
// DOM environment has to provide beyond `document`: `window.matchMedia` (which
// jsdom does not implement) and resize events. If the environment loses either,
// this file is where it shows.
//
// Storage is the exception and comes from a stub, because no environment can
// win it: Node defines a `localStorage` of its own that is unusable without
// `--localstorage-file`, and vitest populates a DOM by skipping every global
// Node has already defined — so happy-dom's storage never lands, on any Node
// new enough to ship one (26 here).

import { act, cleanup, renderHook } from "@testing-library/react";
import type { Window as HappyDOMWindow } from "happy-dom";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { useTermKeyBar } from "./termKeyBar";
import { stubLocalStorage } from "../../lib/shared/fakeStorage";
import { KEYBOARD_MIN_VIEWPORT_PX } from "../../lib/terminal/termKeys";

const WIDE = KEYBOARD_MIN_VIEWPORT_PX + 300;

function resizeTo(width: number) {
  // The environment's own handle: the global `Window` type does not know it.
  (window as unknown as HappyDOMWindow).happyDOM.setViewport({ width });
  window.dispatchEvent(new Event("resize"));
}

// A fresh store per test, which also keeps the stub from outliving the file.
beforeEach(stubLocalStorage);

afterEach(() => {
  localStorage.clear();
  // Vitest runs without globals, so RTL cannot auto-register its cleanup.
  cleanup();
});

// The hook guards its matchMedia use, so its tests alone would keep passing in
// an environment that lost it. This is the assertion the header promises.
it("the environment provides matchMedia and follows the viewport", () => {
  expect(typeof window.matchMedia).toBe("function");
  resizeTo(WIDE);
  expect(window.matchMedia(`(min-width: ${WIDE}px)`).matches).toBe(true);
  resizeTo(WIDE - 1);
  expect(window.matchMedia(`(min-width: ${WIDE}px)`).matches).toBe(false);
});

describe("useTermKeyBar", () => {
  it("defaults to off on wide screens and turns on when the viewport narrows", () => {
    resizeTo(WIDE);
    const { result } = renderHook(() => useTermKeyBar());
    expect(result.current.shown).toBe(false);

    act(() => resizeTo(KEYBOARD_MIN_VIEWPORT_PX - 1));
    expect(result.current.shown).toBe(true);
  });

  it("toggle is saved and is not reversed by resizing", () => {
    // Rotation is a resize: after the user chooses, the device default must not override it.
    resizeTo(WIDE);
    const { result } = renderHook(() => useTermKeyBar());

    act(() => result.current.toggle());
    expect(result.current.shown).toBe(true);
    expect(localStorage.getItem("nightcrow.termKeyBar")).toBe("shown");

    act(() => resizeTo(KEYBOARD_MIN_VIEWPORT_PX - 1));
    act(() => resizeTo(WIDE));
    expect(result.current.shown).toBe(true);
  });

  it("a saved choice overrides the default on the next mount", () => {
    resizeTo(WIDE);
    localStorage.setItem("nightcrow.termKeyBar", "shown");
    const { result } = renderHook(() => useTermKeyBar());
    expect(result.current.shown).toBe(true);
  });

  it("an unknown saved value falls back to the device default", () => {
    resizeTo(WIDE);
    localStorage.setItem("nightcrow.termKeyBar", "definitely-not-a-pref");
    const { result } = renderHook(() => useTermKeyBar());
    expect(result.current.shown).toBe(false);
  });
});
