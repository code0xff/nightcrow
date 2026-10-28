// @vitest-environment happy-dom

import { afterEach, describe, expect, it, vi } from "vitest";
import { fadeOutThen } from "./motion";

afterEach(() => vi.restoreAllMocks());

function reducedMotion(reduce: boolean) {
  vi.spyOn(window, "matchMedia").mockImplementation(
    (query: string) => ({ matches: reduce && query.includes("reduce") }) as MediaQueryList,
  );
}

describe("fadeOutThen", () => {
  it("runs immediately when there is no element", () => {
    const done = vi.fn();
    fadeOutThen(null, done);
    expect(done).toHaveBeenCalledTimes(1);
  });

  it("runs immediately without animation when reduced motion is enabled", () => {
    reducedMotion(true);
    const el = document.createElement("div");
    el.animate = vi.fn();
    const done = vi.fn();
    fadeOutThen(el, done);
    expect(el.animate).not.toHaveBeenCalled();
    expect(done).toHaveBeenCalledTimes(1);
  });

  it("runs after animation ends and only once if end and cancel overlap", () => {
    reducedMotion(false);
    const animation = {} as Animation;
    const el = document.createElement("div");
    el.animate = vi.fn(() => animation);
    const done = vi.fn();
    fadeOutThen(el, done);
    expect(done).not.toHaveBeenCalled();
    animation.onfinish?.call(animation, {} as AnimationPlaybackEvent);
    animation.oncancel?.call(animation, {} as AnimationPlaybackEvent);
    expect(done).toHaveBeenCalledTimes(1);
  });
});
