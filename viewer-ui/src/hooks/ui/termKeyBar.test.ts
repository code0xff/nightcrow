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
import { stubLocalStorage } from "../../lib/fakeStorage";
import { KEYBOARD_MIN_VIEWPORT_PX } from "../../lib/termKeys";

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
it("환경이_matchMedia를_제공하고_뷰포트를_따라간다", () => {
  expect(typeof window.matchMedia).toBe("function");
  resizeTo(WIDE);
  expect(window.matchMedia(`(min-width: ${WIDE}px)`).matches).toBe(true);
  resizeTo(WIDE - 1);
  expect(window.matchMedia(`(min-width: ${WIDE}px)`).matches).toBe(false);
});

describe("useTermKeyBar", () => {
  it("넓은_화면은_기본_꺼짐이고_좁아지면_켜진다", () => {
    resizeTo(WIDE);
    const { result } = renderHook(() => useTermKeyBar());
    expect(result.current.shown).toBe(false);

    act(() => resizeTo(KEYBOARD_MIN_VIEWPORT_PX - 1));
    expect(result.current.shown).toBe(true);
  });

  it("토글은_저장되고_리사이즈에_뒤집히지_않는다", () => {
    // 회전이 곧 리사이즈다: 사람이 정한 뒤에는 기기 기본값이 다시 이기면 안 된다.
    resizeTo(WIDE);
    const { result } = renderHook(() => useTermKeyBar());

    act(() => result.current.toggle());
    expect(result.current.shown).toBe(true);
    expect(localStorage.getItem("nightcrow.termKeyBar")).toBe("shown");

    act(() => resizeTo(KEYBOARD_MIN_VIEWPORT_PX - 1));
    act(() => resizeTo(WIDE));
    expect(result.current.shown).toBe(true);
  });

  it("저장된_선택이_다음_마운트의_기본값을_이긴다", () => {
    resizeTo(WIDE);
    localStorage.setItem("nightcrow.termKeyBar", "shown");
    const { result } = renderHook(() => useTermKeyBar());
    expect(result.current.shown).toBe(true);
  });

  it("모르는_저장값은_기기_기본값으로_돌아간다", () => {
    resizeTo(WIDE);
    localStorage.setItem("nightcrow.termKeyBar", "definitely-not-a-pref");
    const { result } = renderHook(() => useTermKeyBar());
    expect(result.current.shown).toBe(false);
  });
});
