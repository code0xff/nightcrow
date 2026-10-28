// @vitest-environment happy-dom

import { act, cleanup, render } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import type { KeyboardWindowLike } from "../../lib/ui/visualViewport";
import { useSoftKeyboardOpen } from "./useSoftKeyboard";

afterEach(cleanup);

function fakeWindow(innerHeight: number, visibleHeight: number) {
  const listeners = new Set<() => void>();
  const viewport = {
    height: visibleHeight,
    offsetTop: 0,
    addEventListener: (_type: string, listener: () => void) => {
      listeners.add(listener);
    },
    removeEventListener: (_type: string, listener: () => void) => {
      listeners.delete(listener);
    },
  };
  const viewportWindow: KeyboardWindowLike = {
    innerHeight,
    visualViewport: viewport,
    addEventListener(_type, listener) {
      listeners.add(listener);
    },
    removeEventListener(_type, listener) {
      listeners.delete(listener);
    },
  };
  return {
    viewportWindow,
    shrinkTo(height: number) {
      viewport.height = height;
      for (const listener of listeners) listener();
    },
    listenerCount: () => listeners.size,
  };
}

function Probe({
  viewportWindow,
  seen,
}: {
  viewportWindow: KeyboardWindowLike;
  seen: boolean[];
}) {
  seen.push(useSoftKeyboardOpen(viewportWindow));
  return null;
}

describe("useSoftKeyboardOpen", () => {
  it("opens when the viewport shrinks by the keyboard height and closes when it returns", () => {
    const fake = fakeWindow(800, 800);
    const seen: boolean[] = [];
    render(<Probe viewportWindow={fake.viewportWindow} seen={seen} />);
    expect(seen.at(-1)).toBe(false);

    act(() => fake.shrinkTo(480));
    expect(seen.at(-1)).toBe(true);

    act(() => fake.shrinkTo(800));
    expect(seen.at(-1)).toBe(false);
  });

  it("removes the subscription on unmount", () => {
    const fake = fakeWindow(800, 800);
    const view = render(
      <Probe viewportWindow={fake.viewportWindow} seen={[]} />,
    );
    expect(fake.listenerCount()).toBeGreaterThan(0);
    view.unmount();
    expect(fake.listenerCount()).toBe(0);
  });
});
