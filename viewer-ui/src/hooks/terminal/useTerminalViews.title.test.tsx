// @vitest-environment happy-dom

import { act, cleanup, renderHook } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { PaneView } from "../../lib/terminal/terminalLayout";
import { createTerminalReplayState } from "../../lib/terminal/terminalReplay";
import { TITLE_THROTTLE_MS } from "../../lib/terminal/titleThrottle";
import { useTerminalViews } from "./useTerminalViews";

const disposeTracked: (() => void)[] = [];

afterEach(() => {
  cleanup();
  disposeTracked.splice(0).forEach((dispose) => dispose());
  vi.useRealTimers();
});

function write(term: { write(data: string, done: () => void): void }, data: string) {
  return act(
    () =>
      new Promise<void>((done) => {
        term.write(data, done);
      }),
  );
}

async function settle(ms: number) {
  await act(async () => {
    await new Promise<void>((done) => setTimeout(done, ms));
  });
}

describe("useTerminalViews titles", () => {
  it("throttles a burst and keeps the last title without re-allocating on repeats", async () => {
    const body = document.createElement("div");
    document.body.append(body);
    const viewsRef = { current: new Map<number, PaneView>() };
    let titles: Record<number, string> = {};
    let commits = 0;
    const setTitles: React.Dispatch<React.SetStateAction<Record<number, string>>> = (update) => {
      const next = typeof update === "function" ? update(titles) : update;
      if (next !== titles) commits += 1;
      titles = next;
    };

    renderHook(() =>
      useTerminalViews({
        panes: [1],
        size: { w: 600, h: 300 },
        zoomed: null,
        mode: "grid",
        screenScale: 100,
        socketRef: { current: null },
        viewsRef,
        bodyRefs: { current: new Map([[1, body]]) },
        pendingRef: { current: new Map() },
        replayRef: { current: createTerminalReplayState() },
        ptySizesRef: { current: new Map() },
        consumeLatches: (typed: string) => typed,
        setTitles,
      }),
    );
    const term = viewsRef.current.get(1)!.term;
    disposeTracked.push(() => term.dispose());

    await write(term, "\x1b]0;build\x07");
    expect(titles).toEqual({ 1: "build" });
    expect(commits).toBe(1);

    await write(term, "\x1b]0;one\x07\x1b]0;two\x07\x1b]0;three\x07");
    // Inside the window only the pending latest waits; nothing is lost.
    expect(titles).toEqual({ 1: "build" });
    expect(commits).toBe(1);

    await settle(TITLE_THROTTLE_MS + 50);
    expect(titles).toEqual({ 1: "three" });
    expect(commits).toBe(2);

    const held = titles;
    await write(term, "\x1b]0;three\x07");
    await settle(TITLE_THROTTLE_MS + 50);
    expect(titles).toBe(held);
    expect(commits).toBe(2);
    body.remove();
  });
});
