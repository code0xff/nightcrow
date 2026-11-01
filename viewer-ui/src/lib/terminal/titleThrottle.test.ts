import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { createTitleThrottle, TITLE_THROTTLE_MS } from "./titleThrottle";

beforeEach(() => {
  vi.useFakeTimers();
  vi.setSystemTime(1_000_000);
});

afterEach(() => {
  vi.useRealTimers();
});

describe("createTitleThrottle", () => {
  it("delivers the first title at once", () => {
    const deliver = vi.fn();
    createTitleThrottle(deliver).push("first");
    expect(deliver).toHaveBeenCalledTimes(1);
    expect(deliver).toHaveBeenCalledWith("first");
  });

  it("collapses a burst to the latest title and keeps the trailing one", () => {
    const deliver = vi.fn();
    const throttle = createTitleThrottle(deliver);
    throttle.push("first");
    throttle.push("second");
    throttle.push("third");
    expect(deliver).toHaveBeenCalledTimes(1);

    vi.advanceTimersByTime(TITLE_THROTTLE_MS);
    expect(deliver).toHaveBeenCalledTimes(2);
    expect(deliver).toHaveBeenLastCalledWith("third");
  });

  it("does not repeat the title already on screen", () => {
    const deliver = vi.fn();
    const throttle = createTitleThrottle(deliver);
    throttle.push("same");
    vi.advanceTimersByTime(TITLE_THROTTLE_MS);
    throttle.push("same");
    vi.advanceTimersByTime(TITLE_THROTTLE_MS);
    expect(deliver).toHaveBeenCalledTimes(1);
  });

  it("returns to a quiet period with an immediate title", () => {
    const deliver = vi.fn();
    const throttle = createTitleThrottle(deliver);
    throttle.push("first");
    vi.advanceTimersByTime(TITLE_THROTTLE_MS);
    throttle.push("second");
    expect(deliver).toHaveBeenCalledTimes(2);
    expect(deliver).toHaveBeenLastCalledWith("second");
  });

  it("drops a pending title that the delivered one made moot", () => {
    const deliver = vi.fn();
    const throttle = createTitleThrottle(deliver);
    throttle.push("first");
    throttle.push("second");
    throttle.push("first");
    vi.advanceTimersByTime(TITLE_THROTTLE_MS);
    expect(deliver).toHaveBeenCalledTimes(1);
  });

  it("cancel drops the scheduled delivery", () => {
    const deliver = vi.fn();
    const throttle = createTitleThrottle(deliver);
    throttle.push("first");
    throttle.push("second");
    throttle.cancel();
    vi.advanceTimersByTime(TITLE_THROTTLE_MS);
    expect(deliver).toHaveBeenCalledTimes(1);
  });
});
