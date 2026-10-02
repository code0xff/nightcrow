// @vitest-environment happy-dom

import { act, cleanup, renderHook } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { PaneView } from "../../lib/terminal/terminalLayout";
import { usePaneSizes } from "./usePaneSizes";

afterEach(() => {
  cleanup();
  vi.useRealTimers();
});

beforeEach(() => vi.useFakeTimers());

function setup(overrides: Partial<Parameters<typeof usePaneSizes>[0]> = {}) {
  const body = document.createElement("div");
  Object.defineProperties(body, {
    clientWidth: { value: 500 },
    clientHeight: { value: 240 },
  });
  const fit = { fit: vi.fn() };
  const term = { rows: 24, cols: 80, resize: vi.fn() };
  const viewsRef = { current: new Map([[1, { term, fit } as unknown as PaneView]]) };
  const bodyRefs = { current: new Map([[1, body]]) };
  const askedSizesRef = { current: new Map([[1, { rows: 24, cols: 80 }]]) };
  const desiredSizesRef = { current: new Map<number, { rows: number; cols: number }>() };
  const send = vi.fn();
  const socket = { readyState: 1, send } as unknown as WebSocket;
  const args: Parameters<typeof usePaneSizes>[0] = {
    panes: [1],
    size: { w: 500, h: 240 },
    zoomed: null,
    mode: "grid",
    screenScale: 100,
    socketRef: { current: socket },
    viewsRef,
    bodyRefs,
    ptySizesRef: { current: new Map([[1, { rows: 24, cols: 80 }]]) },
    desiredSizesRef,
    askedSizesRef,
    ownsSize: true,
    layoutPending: false,
    keyboardOpen: false,
    refitEpoch: 0,
    ...overrides,
  };
  return { args, fit, term, send, askedSizesRef, desiredSizesRef };
}

describe("usePaneSizes", () => {
  it("manual_refit_resends_the_grid_even_when_the_dimensions_match", () => {
    const { args, fit, send } = setup();
    const { rerender } = renderHook(({ epoch }) =>
      usePaneSizes({ ...args, refitEpoch: epoch }),
    { initialProps: { epoch: 0 } });
    act(() => vi.advanceTimersByTime(60));
    expect(fit.fit).toHaveBeenCalledTimes(1);
    expect(send).not.toHaveBeenCalled();

    rerender({ epoch: 1 });
    act(() => vi.advanceTimersByTime(60));
    expect(fit.fit).toHaveBeenCalledTimes(2);
    expect(send).toHaveBeenCalledTimes(1);
    expect(JSON.parse(send.mock.calls[0][0]))
      .toEqual({ type: "resize", pane: 1, rows: 24, cols: 80 });
  });

  it("soft_keyboard_open_holds_resize_until_the_geometry_returns", () => {
    const { args, fit, term, send } = setup({ keyboardOpen: true });
    const { rerender } = renderHook(({ keyboardOpen }) =>
      usePaneSizes({ ...args, keyboardOpen }),
    { initialProps: { keyboardOpen: true } });
    act(() => vi.advanceTimersByTime(100));
    expect(fit.fit).not.toHaveBeenCalled();
    expect(send).not.toHaveBeenCalled();

    term.rows = 20;
    term.cols = 60;
    rerender({ keyboardOpen: false });
    act(() => vi.advanceTimersByTime(60));
    expect(fit.fit).toHaveBeenCalledTimes(1);
    expect(send).toHaveBeenCalledTimes(1);
  });

  it("sends_the_new_fit_after_an_old_resize_ack_overwrites_xterms_rows", () => {
    const { args, fit, term, send } = setup();
    fit.fit.mockImplementation(() => {
      term.rows = 20;
      term.cols = 60;
    });
    renderHook(() => usePaneSizes(args));

    term.rows = 24;
    term.cols = 80;
    act(() => vi.advanceTimersByTime(60));

    expect(send).toHaveBeenCalledTimes(1);
    expect(JSON.parse(send.mock.calls[0][0])).toEqual({
      type: "resize",
      pane: 1,
      rows: 20,
      cols: 60,
    });
  });
});
