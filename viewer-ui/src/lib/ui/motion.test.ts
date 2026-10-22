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
  it("요소가_없으면_바로_실행한다", () => {
    const done = vi.fn();
    fadeOutThen(null, done);
    expect(done).toHaveBeenCalledTimes(1);
  });

  it("동작_줄이기면_애니메이션_없이_바로_실행한다", () => {
    reducedMotion(true);
    const el = document.createElement("div");
    el.animate = vi.fn();
    const done = vi.fn();
    fadeOutThen(el, done);
    expect(el.animate).not.toHaveBeenCalled();
    expect(done).toHaveBeenCalledTimes(1);
  });

  it("애니메이션이_끝나야_실행하고_끝과_취소가_겹쳐도_한_번만_실행한다", () => {
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
