// @vitest-environment happy-dom

import { act, cleanup, renderHook } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { Terminal } from "@xterm/xterm";
import { createTerminalReplayState } from "../../lib/terminal/terminalReplay";
import type { PaneView } from "../../lib/terminal/terminalLayout";
import { RECONNECT_BASE_MS, RECONNECT_MAX_MS, RECONNECT_STABLE_MS } from "../../lib/terminal/reconnect";
import { useTerminalSocket } from "./useTerminalSocket";

class MockWebSocket {
  static instances: MockWebSocket[] = [];
  static OPEN = 1;
  readyState = MockWebSocket.OPEN;
  binaryType = "blob";
  onopen: ((event: Event) => void) | null = null;
  onmessage: ((event: MessageEvent) => void) | null = null;
  onclose: ((event: CloseEvent) => void) | null = null;
  close = vi.fn();

  constructor(readonly url: string) {
    MockWebSocket.instances.push(this);
  }
}

function stableArgs() {
  return {
    socketRef: { current: null },
    viewsRef: { current: new Map() },
    pendingRef: { current: new Map() },
    replayRef: { current: createTerminalReplayState() },
    ptySizesRef: { current: new Map() },
    askedSizesRef: { current: new Map() },
    desiredSizesRef: { current: new Map() },
    ownsSizeRef: { current: false },
    sizeOwnerGenerationRef: { current: null },
    onSizeAcquired: vi.fn(),
    zoomAskedRef: { current: undefined },
    setLink: vi.fn(),
    setPending: vi.fn(),
    setReplayLeft: vi.fn(),
    setPanes: vi.fn(),
    setActive: vi.fn(),
    setZoomed: vi.fn(),
    setTitles: vi.fn(),
    setOwnsSize: vi.fn(),
    setRecovery: vi.fn(),
  } satisfies Omit<Parameters<typeof useTerminalSocket>[0], "repo">;
}

/** A pane whose emulator reports what happened to it, in order. */
function fakePane() {
  const events: string[] = [];
  const term = {
    rows: 24,
    cols: 80,
    write: (data: string | Uint8Array, callback?: () => void) => {
      events.push(typeof data === "string" ? `write:${data}` : "write:bytes");
      callback?.();
    },
    reset: vi.fn(() => events.push("reset")),
    resize: vi.fn(),
    dispose: vi.fn(() => events.push("dispose")),
  } as unknown as Terminal;
  const view = {
    term,
    fit: {},
    replayGate: { begin: vi.fn(), release: vi.fn() },
  } as unknown as PaneView;
  return { view, term, events };
}

function outputFrame(pane: number, data: string): ArrayBuffer {
  const bytes = new TextEncoder().encode(data);
  const frame = new ArrayBuffer(bytes.length + 4);
  new DataView(frame).setUint32(0, pane, true);
  new Uint8Array(frame, 4).set(bytes);
  return frame;
}

function closeCurrent(): void {
  const socket = MockWebSocket.instances.at(-1)!;
  act(() => socket.onclose?.({} as CloseEvent));
}

afterEach(() => {
  cleanup();
  vi.useRealTimers();
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
  MockWebSocket.instances = [];
});

describe("useTerminalSocket lifecycle", () => {
  it("keeps its connection on an ordinary rerender and reconnects on a repository change", () => {
    vi.stubGlobal("WebSocket", MockWebSocket);
    const args = stableArgs();
    const { rerender, unmount } = renderHook(
      ({ repo }) => useTerminalSocket({ ...args, repo }),
      { initialProps: { repo: "repo-a" } },
    );
    const first = MockWebSocket.instances[0];

    rerender({ repo: "repo-a" });
    expect(MockWebSocket.instances).toHaveLength(1);

    rerender({ repo: "repo-b" });
    expect(MockWebSocket.instances).toHaveLength(2);
    expect(first.close).toHaveBeenCalledOnce();
    expect(args.socketRef.current).toBe(MockWebSocket.instances[1]);

    unmount();
    expect(MockWebSocket.instances[1].close).toHaveBeenCalledOnce();
  });

  it("backs off from half a second to the ceiling after a lost connection", () => {
    vi.useFakeTimers();
    vi.spyOn(Math, "random").mockReturnValue(0.5);
    vi.stubGlobal("WebSocket", MockWebSocket);
    renderHook(() => useTerminalSocket({ ...stableArgs(), repo: "repo" }));

    const delays = [500, 1_000, 2_000, 4_000, 8_000, RECONNECT_MAX_MS];
    for (const delay of delays) {
      const before = MockWebSocket.instances.length;
      closeCurrent();
      act(() => vi.advanceTimersByTime(delay - 1));
      expect(MockWebSocket.instances).toHaveLength(before);
      act(() => vi.advanceTimersByTime(1));
      expect(MockWebSocket.instances).toHaveLength(before + 1);
    }
  });

  it("starts over from the base wait once a connection has stayed up", () => {
    vi.useFakeTimers();
    vi.spyOn(Math, "random").mockReturnValue(0.5);
    vi.stubGlobal("WebSocket", MockWebSocket);
    renderHook(() => useTerminalSocket({ ...stableArgs(), repo: "repo" }));

    closeCurrent();
    act(() => vi.advanceTimersByTime(500));
    closeCurrent();
    act(() => vi.advanceTimersByTime(1_000));
    expect(MockWebSocket.instances).toHaveLength(3);

    const stable = MockWebSocket.instances[2];
    act(() => stable.onopen?.(new Event("open")));
    act(() => vi.advanceTimersByTime(RECONNECT_STABLE_MS));
    closeCurrent();

    const before = MockWebSocket.instances.length;
    act(() => vi.advanceTimersByTime(RECONNECT_BASE_MS - 1));
    expect(MockWebSocket.instances).toHaveLength(before);
    act(() => vi.advanceTimersByTime(1));
    expect(MockWebSocket.instances).toHaveLength(before + 1);
  });

  it("reuses the pane's emulator across a reconnect and resets it before the replay", () => {
    vi.useFakeTimers();
    vi.spyOn(Math, "random").mockReturnValue(0.5);
    vi.stubGlobal("WebSocket", MockWebSocket);
    const args = stableArgs();
    const pane = fakePane();
    renderHook(() => useTerminalSocket({ ...args, repo: "repo" }));
    // The panel opens its emulators after this effect has subscribed.
    args.viewsRef.current.set(7, pane.view);

    closeCurrent();
    act(() => vi.advanceTimersByTime(500));
    expect(pane.term.dispose).not.toHaveBeenCalled();
    expect(pane.events).toEqual(["write:", "reset"]);

    const socket = MockWebSocket.instances[1];
    act(() =>
      socket.onmessage?.({
        data: JSON.stringify({ type: "hello", client: 3, panes: 1 }),
      } as MessageEvent),
    );
    act(() =>
      socket.onmessage?.({
        data: JSON.stringify({ type: "created", pane: 7, rows: 24, cols: 80 }),
      } as MessageEvent),
    );
    act(() =>
      socket.onmessage?.({ data: outputFrame(7, "replayed") } as MessageEvent),
    );
    act(() =>
      socket.onmessage?.({
        data: JSON.stringify({ type: "replay_complete" }),
      } as MessageEvent),
    );

    expect(args.viewsRef.current.get(7)).toBe(pane.view);
    expect(pane.term.reset).toHaveBeenCalledOnce();
    expect(pane.events.indexOf("reset")).toBeLessThan(
      pane.events.indexOf("write:bytes"),
    );
  });
});
