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
  it("calls the handler for each document keydown", () => {
    const handler = vi.fn(() => false);
    renderHook(() => useGlobalKeydown(handler));

    press(document.body);

    expect(handler).toHaveBeenCalledTimes(1);
  });

  it("returning true stops child listeners and prevents the default action", () => {
    const { el, seen } = child();
    renderHook(() => useGlobalKeydown(() => true));

    const event = press(el);

    expect(seen).not.toHaveBeenCalled();
    expect(event.defaultPrevented).toBe(true);
  });

  it("returning false leaves the event untouched", () => {
    const { el, seen } = child();
    renderHook(() => useGlobalKeydown(() => false));

    const event = press(el);

    expect(seen).toHaveBeenCalledTimes(1);
    expect(event.defaultPrevented).toBe(false);
  });

  it("removes the listener on unmount", () => {
    const handler = vi.fn(() => false);
    const { unmount } = renderHook(() => useGlobalKeydown(handler));

    unmount();
    press(document.body);

    expect(handler).not.toHaveBeenCalled();
  });

  it("registers nothing when enabled is false", () => {
    const handler = vi.fn(() => true);
    const { el, seen } = child();
    renderHook(() => useGlobalKeydown(handler, false));

    const event = press(el);

    expect(handler).not.toHaveBeenCalled();
    expect(seen).toHaveBeenCalledTimes(1);
    expect(event.defaultPrevented).toBe(false);
  });

  it("removes the listener when enabled changes to false", () => {
    const handler = vi.fn(() => false);
    const { rerender } = renderHook(
      ({ on }: { on: boolean }) => useGlobalKeydown(handler, on),
      { initialProps: { on: true } },
    );

    rerender({ on: false });
    press(document.body);

    expect(handler).not.toHaveBeenCalled();
  });

  it("does not resubscribe when the handler changes and calls the latest handler", () => {
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
