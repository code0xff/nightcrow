// @vitest-environment happy-dom

import { act, cleanup, renderHook } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { Terminal } from "@xterm/xterm";
import { FitAddon } from "@xterm/addon-fit";
import type { PaneView } from "../../lib/terminal/terminalLayout";
import { usePaneSizes } from "./usePaneSizes";

const disposeRealTerminals: (() => void)[] = [];

afterEach(() => {
  cleanup();
  disposeRealTerminals.splice(0).forEach((dispose) => dispose());
  vi.useRealTimers();
  vi.unstubAllGlobals();
  vi.restoreAllMocks();
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

  it("remeasures hidden xterm metrics at the same grid before fitting", () => {
    let measurable = false;
    vi.stubGlobal("OffscreenCanvas", undefined);
    vi.spyOn(HTMLElement.prototype, "offsetWidth", "get").mockImplementation(
      function (this: HTMLElement) {
        return this.classList.contains("xterm-char-measure-element") && measurable
          ? 256
          : 0;
      },
    );
    vi.spyOn(HTMLElement.prototype, "offsetHeight", "get").mockImplementation(
      function (this: HTMLElement) {
        return this.classList.contains("xterm-char-measure-element") && measurable
          ? 16
          : 0;
      },
    );

    const body = document.createElement("div");
    body.style.width = "656px";
    body.style.height = "384px";
    const dimensions = { width: 0, height: 0 };
    Object.defineProperties(body, {
      clientWidth: { get: () => dimensions.width },
      clientHeight: { get: () => dimensions.height },
    });
    document.body.append(body);

    const term = new Terminal();
    const fit = new FitAddon();
    term.loadAddon(fit);
    term.open(body);
    term.element!.style.padding = "0px";
    const resizes: unknown[] = [];
    term.onResize((size) => resizes.push(size));
    const resize = vi.spyOn(term, "resize");
    disposeRealTerminals.push(() => term.dispose());
    expect(fit.proposeDimensions()).toBeUndefined();

    const socket = { readyState: WebSocket.OPEN, send: vi.fn() } as unknown as WebSocket;
    const viewsRef = {
      current: new Map([[1, { term, fit } as unknown as PaneView]]),
    };
    const bodyRefs = { current: new Map([[1, body]]) };
    const args: Parameters<typeof usePaneSizes>[0] = {
      panes: [1],
      size: { w: 656, h: 384 },
      zoomed: null,
      mode: "grid",
      screenScale: 100,
      socketRef: { current: socket },
      viewsRef,
      bodyRefs,
      ptySizesRef: { current: new Map([[1, { rows: 24, cols: 80 }]]) },
      desiredSizesRef: { current: new Map() },
      askedSizesRef: { current: new Map([[1, { rows: 24, cols: 80 }]]) },
      ownsSize: true,
      layoutPending: false,
      keyboardOpen: false,
      refitEpoch: 0,
    };
    const { rerender } = renderHook(
      ({ size }) => usePaneSizes({ ...args, size }),
      { initialProps: { size: args.size } },
    );

    measurable = true;
    dimensions.width = 656;
    dimensions.height = 384;
    rerender({ size: { w: 656, h: 384 } });
    expect(resize).toHaveBeenCalledWith(80, 24);
    expect(fit.proposeDimensions()).toEqual({ cols: 80, rows: 24 });
    expect([term.cols, term.rows]).toEqual([80, 24]);
    act(() => vi.advanceTimersByTime(60));
    expect(resizes).toEqual([]);
    expect(socket.send).not.toHaveBeenCalled();
    body.remove();
  });
});
