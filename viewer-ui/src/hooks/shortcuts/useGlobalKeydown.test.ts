// @vitest-environment happy-dom

import { cleanup, renderHook } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { useGlobalKeydown } from "./useGlobalKeydown";

/** A descendant listening in the bubble phase, standing in for xterm. */
function child() {
  const el = document.createElement("textarea");
  document.body.appendChild(el);
  const seen = vi.fn();
  el.addEventListener("keydown", seen);
  return { el, seen };
}

function press(el: Element, key = "ArrowLeft") {
  const event = new KeyboardEvent("keydown", {
    key,
    bubbles: true,
    cancelable: true,
  });
  el.dispatchEvent(event);
  return event;
}

// Vitest runs without globals, so RTL cannot auto-register its cleanup.
afterEach(() => {
  cleanup();
  document.body.innerHTML = "";
});

describe("useGlobalKeydown", () => {
  it("document의_keydown마다_핸들러가_불린다", () => {
    const handler = vi.fn(() => false);
    renderHook(() => useGlobalKeydown(handler));

    press(document.body);

    expect(handler).toHaveBeenCalledTimes(1);
  });

  it("true를_반환하면_자식_리스너까지_막고_기본_동작도_막는다", () => {
    const { el, seen } = child();
    renderHook(() => useGlobalKeydown(() => true));

    const event = press(el);

    expect(seen).not.toHaveBeenCalled();
    expect(event.defaultPrevented).toBe(true);
  });

  it("false를_반환하면_이벤트를_건드리지_않는다", () => {
    const { el, seen } = child();
    renderHook(() => useGlobalKeydown(() => false));

    const event = press(el);

    expect(seen).toHaveBeenCalledTimes(1);
    expect(event.defaultPrevented).toBe(false);
  });

  it("언마운트하면_리스너가_사라진다", () => {
    const handler = vi.fn(() => false);
    const { unmount } = renderHook(() => useGlobalKeydown(handler));

    unmount();
    press(document.body);

    expect(handler).not.toHaveBeenCalled();
  });

  it("enabled가_false면_아무것도_등록하지_않는다", () => {
    const handler = vi.fn(() => true);
    const { el, seen } = child();
    renderHook(() => useGlobalKeydown(handler, false));

    const event = press(el);

    expect(handler).not.toHaveBeenCalled();
    expect(seen).toHaveBeenCalledTimes(1);
    expect(event.defaultPrevented).toBe(false);
  });

  it("enabled가_false로_바뀌면_리스너를_뗀다", () => {
    const handler = vi.fn(() => false);
    const { rerender } = renderHook(
      ({ on }: { on: boolean }) => useGlobalKeydown(handler, on),
      { initialProps: { on: true } },
    );

    rerender({ on: false });
    press(document.body);

    expect(handler).not.toHaveBeenCalled();
  });

  it("핸들러가_바뀌어도_다시_구독하지_않고_최신_것이_불린다", () => {
    const first = vi.fn(() => false);
    const second = vi.fn(() => false);
    const add = vi.spyOn(document, "addEventListener");
    const { rerender } = renderHook(
      ({ h }: { h: () => boolean }) => useGlobalKeydown(h),
      { initialProps: { h: first } },
    );
    const subscribed = add.mock.calls.length;

    rerender({ h: second });
    press(document.body);

    expect(add.mock.calls.length).toBe(subscribed);
    expect(first).not.toHaveBeenCalled();
    expect(second).toHaveBeenCalledTimes(1);
    add.mockRestore();
  });
});
