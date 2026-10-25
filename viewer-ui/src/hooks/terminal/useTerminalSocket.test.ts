// @vitest-environment happy-dom

import { cleanup, renderHook } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { useTerminalSocket } from "./useTerminalSocket";

class MockWebSocket {
  static instances: MockWebSocket[] = [];
  static OPEN = 1;
  readyState = MockWebSocket.OPEN;
  binaryType = "blob";
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
    replayRef: { current: { active: false, panes: new Map() } },
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

afterEach(() => {
  cleanup();
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
});
