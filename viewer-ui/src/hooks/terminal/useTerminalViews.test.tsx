// @vitest-environment happy-dom

import { act, cleanup, renderHook } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { Terminal } from "@xterm/xterm";
import type { PaneView } from "../../lib/terminal/terminalLayout";
import {
  createTerminalReplayState,
  type PendingTerminalWrite,
} from "../../lib/terminal/terminalReplay";
import type { TerminalMessageContext } from "./terminalSocketMessages";
import { handleTerminalSocketMessage } from "./terminalSocketMessages";
import { useCursorInView } from "./useCursorInView";
import { useTerminalViews } from "./useTerminalViews";

const disposeTracked: (() => void)[] = [];

afterEach(() => {
  cleanup();
  disposeTracked.splice(0).forEach((dispose) => dispose());
  vi.unstubAllGlobals();
});

function track(term: Terminal) {
  let disposed = false;
  const original = term.dispose.bind(term);
  const dispose = vi.spyOn(term, "dispose").mockImplementation(() => {
    if (disposed) return;
    disposed = true;
    original();
  });
  disposeTracked.push(() => term.dispose());
  return dispose;
}

function output(pane: number, data: string): ArrayBuffer {
  const bytes = new TextEncoder().encode(data);
  const frame = new ArrayBuffer(bytes.length + 4);
  new DataView(frame).setUint32(0, pane, true);
  new Uint8Array(frame, 4).set(bytes);
  return frame;
}

function control(context: TerminalMessageContext, message: object): void {
  handleTerminalSocketMessage(JSON.stringify(message), context);
}

function write(term: Terminal, data = ""): Promise<void> {
  return new Promise((resolve) => term.write(data, resolve));
}

function bufferLines(term: Terminal): string[] {
  const buffer = term.buffer.active;
  return Array.from({ length: buffer.length }, (_, row) =>
    buffer.getLine(row)?.translateToString(true) ?? "",
  );
}

describe("useTerminalViews", () => {
  it("parses hidden replay and live output without reopening on reveal", async () => {
    const pane = 7;
    const dimensions = { width: 0, height: 0 };
    const body = document.createElement("div");
    Object.defineProperties(body, {
      clientWidth: { get: () => dimensions.width },
      clientHeight: { get: () => dimensions.height },
    });
    document.body.append(body);

    const viewsRef = { current: new Map<number, PaneView>() };
    const pendingRef = {
      current: new Map<number, PendingTerminalWrite[]>(),
    };
    const replayRef = { current: createTerminalReplayState() };
    const ptySizesRef = { current: new Map() };
    const socketSend = vi.fn();
    const socket = { readyState: WebSocket.OPEN, send: socketSend } as unknown as WebSocket;
    const observe = vi.fn();
    vi.stubGlobal(
      "ResizeObserver",
      class {
        observe = observe;
        disconnect = vi.fn();
      },
    );
    const context = {
      repo: "test",
      clientIdRef: { current: null },
      viewsRef,
      pendingRef,
      replayRef,
      ptySizesRef,
      askedSizesRef: { current: new Map() },
      desiredSizesRef: { current: new Map() },
      ownsSizeRef: { current: false },
      sizeOwnerGenerationRef: { current: null },
      onSizeAcquired: () => {},
      zoomAskedRef: { current: undefined },
      setLink: () => {},
      setPending: () => {},
      setReplayLeft: () => {},
      setPanes: () => {},
      setActive: () => {},
      setZoomed: () => {},
      setTitles: () => {},
      setOwnsSize: () => {},
      setRecovery: () => {},
    } as unknown as TerminalMessageContext;

    control(context, { type: "hello", client: 1, panes: 1 });
    control(context, { type: "created", pane, rows: 5, cols: 40 });
    const replay =
      "\x1b]4;1;#123456;1;?\x07" +
      Array.from({ length: 1200 }, (_, i) => `replay-${i}\r\n`).join("");
    handleTerminalSocketMessage(output(pane, `\x1b[6n${replay}`), context);
    control(context, { type: "replay_complete" });
    expect(pendingRef.current.get(pane)).toHaveLength(2);

    const bodyRefs = { current: new Map([[pane, body]]) };
    const args: Parameters<typeof useTerminalViews>[0] = {
      panes: [pane],
      size: { w: 600, h: 300 },
      zoomed: null,
      mode: "grid" as const,
      screenScale: 100,
      socketRef: { current: socket },
      viewsRef,
      bodyRefs,
      pendingRef,
      replayRef,
      ptySizesRef,
      consumeLatches: (typed: string) => typed,
      setTitles: () => {},
    };
    const { rerender } = renderHook(
      ({ panes, size, zoomed, mode }) => {
        useTerminalViews({ ...args, panes, size, zoomed, mode });
        useCursorInView({
          panes,
          viewsRef,
          bodyRefs,
          keyboardOpen: false,
          size,
          zoomed,
          mode,
        });
      },
      {
        initialProps: {
          panes: args.panes,
          size: args.size,
          zoomed: args.zoomed,
          mode: args.mode,
        },
      },
    );

    const view = viewsRef.current.get(pane)!;
    const term = view.term;
    const dispose = track(term);
    const open = vi.spyOn(term, "open");
    const fit = vi.spyOn(view.fit, "fit");
    const resize = vi.spyOn(term, "resize");
    const replies: string[] = [];
    term.onData((data) => replies.push(data));
    expect(term.element?.parentElement).toBe(body);
    expect([term.cols, term.rows]).toEqual([40, 5]);
    expect(pendingRef.current.has(pane)).toBe(false);
    observe.mockClear();

    await act(async () => write(term));
    expect(replies.join("").match(/\x1b\[\d+;\d+R/g) ?? []).toEqual([]);
    expect(replies.join("")).toContain("\x1b]4;1;rgb:1212/3434/5656");
    expect(replayRef.current.panes.has(pane)).toBe(false);

    replies.length = 0;
    handleTerminalSocketMessage(output(pane, "\x1b[6n"), context);
    await act(async () => write(term));
    expect(replies.join("").match(/\x1b\[\d+;\d+R/g)).not.toBeNull();

    replies.length = 0;
    const colors = "\x1b]4;1;?\x07\x1b]10;?\x07\x1b]11;?\x07\x1b]12;?\x07";
    handleTerminalSocketMessage(
      output(pane, `\x1b]104;1\x07${colors}`),
      context,
    );
    await act(async () => write(term));
    const defaults = replies.join("");
    expect((defaults.match(/\x1b\]/g) ?? [])).toHaveLength(4);

    replies.length = 0;
    const setAndQuery =
      "\x1b]4;1;#123456;1;?\x07" +
      "\x1b]10;#234567\x07\x1b]10;?\x07" +
      "\x1b]11;#345678\x07\x1b]11;?\x07" +
      "\x1b]12;#456789\x07\x1b]12;?\x07";
    handleTerminalSocketMessage(output(pane, setAndQuery), context);
    await act(async () => write(term));
    const changed = replies.join("");
    expect(changed).toContain("rgb:1212/3434/5656");
    expect(changed).toContain("rgb:2323/4545/6767");
    expect(changed).toContain("rgb:3434/5656/7878");
    expect(changed).toContain("rgb:4545/6767/8989");

    replies.length = 0;
    const resets = "\x1b]104;1\x07\x1b]110\x07\x1b]111\x07\x1b]112\x07";
    handleTerminalSocketMessage(output(pane, resets + colors), context);
    await act(async () => write(term));
    expect(replies.join("")).toBe(defaults);
    socketSend.mockClear();

    const live = Array.from({ length: 1200 }, (_, i) => `live-${i}\r\n`).join("");
    handleTerminalSocketMessage(output(pane, live), context);
    expect(pendingRef.current.has(pane)).toBe(false);
    await act(async () => write(term));
    expect(term.buffer.active.length).toBeLessThanOrEqual(
      term.rows + (term.options.scrollback ?? 1000),
    );
    const beforeReveal = bufferLines(term);

    dimensions.width = 600;
    dimensions.height = 180;
    rerender({
      panes: [pane],
      size: args.size,
      zoomed: null,
      mode: "tabs",
    });
    expect(open).not.toHaveBeenCalled();
    expect(observe).toHaveBeenCalledWith(body);
    expect(term.element?.parentElement).toBe(body);
    expect([term.cols, term.rows]).toEqual([40, 5]);
    expect(bufferLines(term)).toEqual(beforeReveal);
    expect(resize).not.toHaveBeenCalled();
    expect(fit).not.toHaveBeenCalled();
    expect(socketSend).not.toHaveBeenCalled();

    rerender({
      panes: [pane],
      size: args.size,
      zoomed: pane,
      mode: "tabs",
    });
    expect(open).not.toHaveBeenCalled();
    rerender({ panes: [], size: args.size, zoomed: pane, mode: "tabs" });
    expect(dispose).toHaveBeenCalledTimes(1);
    expect(viewsRef.current.has(pane)).toBe(false);
    body.remove();
  });
});
