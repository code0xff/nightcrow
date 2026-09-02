// @vitest-environment happy-dom

import { act, cleanup, render } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import type { KeyboardWindowLike } from "../../lib/visualViewport";
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
  it("뷰포트가_키보드_높이만큼_줄면_열림으로_바뀌고_돌아오면_닫힌다", () => {
    const fake = fakeWindow(800, 800);
    const seen: boolean[] = [];
    render(<Probe viewportWindow={fake.viewportWindow} seen={seen} />);
    expect(seen.at(-1)).toBe(false);

    act(() => fake.shrinkTo(480));
    expect(seen.at(-1)).toBe(true);

    act(() => fake.shrinkTo(800));
    expect(seen.at(-1)).toBe(false);
  });

  it("언마운트하면_구독을_거둔다", () => {
    const fake = fakeWindow(800, 800);
    const view = render(
      <Probe viewportWindow={fake.viewportWindow} seen={[]} />,
    );
    expect(fake.listenerCount()).toBeGreaterThan(0);
    view.unmount();
    expect(fake.listenerCount()).toBe(0);
  });
});
