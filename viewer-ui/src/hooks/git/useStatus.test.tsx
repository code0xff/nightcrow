// @vitest-environment happy-dom

import { act, cleanup, renderHook } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { api, subscribeStatus, type Diff, type Status } from "../../api";
import { useStatus } from "./useStatus";

vi.mock("../../api", () => ({
  api: { diff: vi.fn() },
  subscribeStatus: vi.fn(),
}));

const diff: Diff = { path: "a.ts", hunks: [], truncated: false };

afterEach(() => {
  cleanup();
  vi.clearAllMocks();
});

function row(path: string, mtime: number, index = "M") {
  return { path, index, worktree: " ", mtime };
}

function status(files: Status["files"]): Status {
  return { files, truncated: false };
}

function setup(panePath = "a.ts") {
  const channel: { deliver: ((value: Status) => void) | null } = {
    deliver: null,
  };
  vi.mocked(subscribeStatus).mockImplementation((_repo, onStatus) => {
    channel.deliver = onStatus;
    return () => {};
  });
  const setPane = vi.fn();
  const args = {
    repo: "repo-1",
    authed: true,
    resumeTick: 0,
    tab: "status" as const,
    pane: { kind: "diff" as const, value: { ...diff, path: panePath } },
    setPane,
    handle: vi.fn(),
    paneRequestRef: { current: 0 },
  };
  const hook = renderHook(() => useStatus(args));
  const send = async (value: Status) => {
    await act(async () => {
      channel.deliver?.(value);
    });
  };
  return { hook, send, setPane, args };
}

describe("useStatus open diff refresh", () => {
  it("does not refetch the open diff for an unrelated status event", async () => {
    vi.mocked(api.diff).mockResolvedValue(diff);
    const { send } = setup();
    await send(status([row("a.ts", 100), row("b.ts", 100)]));
    vi.mocked(api.diff).mockClear();
    await send(status([row("a.ts", 100), row("b.ts", 200)]));
    expect(api.diff).not.toHaveBeenCalled();
  });

  it("refetches when the open row's status or mtime changes", async () => {
    vi.mocked(api.diff).mockResolvedValue(diff);
    const { send, setPane } = setup();
    await send(status([row("a.ts", 100)]));
    vi.mocked(api.diff).mockClear();
    await send(status([row("a.ts", 200)]));
    expect(api.diff).toHaveBeenCalledTimes(1);
    expect(api.diff).toHaveBeenCalledWith("repo-1", "a.ts");
    expect(setPane).toHaveBeenCalledWith(
      expect.objectContaining({ kind: "diff", value: diff }),
    );

    // The same row reported again is not a new change.
    await send(status([row("a.ts", 200)]));
    expect(api.diff).toHaveBeenCalledTimes(1);

    await send(status([row("a.ts", 200, "D")]));
    expect(api.diff).toHaveBeenCalledTimes(2);
  });

  it("refetches when HEAD moves under an unchanged row", async () => {
    vi.mocked(api.diff).mockResolvedValue(diff);
    const { send } = setup();
    await send({ ...status([row("a.ts", 100)]), head: "aaa" });
    vi.mocked(api.diff).mockClear();
    await send({ ...status([row("a.ts", 100)]), head: "bbb" });
    expect(api.diff).toHaveBeenCalledTimes(1);
  });

  it("empties the pane when the open row disappears", async () => {
    vi.mocked(api.diff).mockResolvedValue(diff);
    const { send, setPane } = setup();
    await send(status([row("a.ts", 100)]));
    await send(status([row("b.ts", 100)]));
    expect(setPane).toHaveBeenCalledWith({ kind: "empty" });
  });

  it("adopts a newly opened diff whose row has not moved since the last event", async () => {
    vi.mocked(api.diff).mockResolvedValue(diff);
    const { hook, send, args } = setup("a.ts");
    await send(status([row("a.ts", 100), row("b.ts", 100)]));
    vi.mocked(api.diff).mockClear();
    args.pane = { kind: "diff", value: { ...diff, path: "b.ts" } };
    hook.rerender();
    await send(status([row("a.ts", 300), row("b.ts", 100)]));
    expect(api.diff).not.toHaveBeenCalled();
  });

  it("refetches a newly opened diff whose row moved after it was opened", async () => {
    vi.mocked(api.diff).mockResolvedValue(diff);
    const { hook, send, args } = setup("a.ts");
    await send(status([row("a.ts", 100), row("b.ts", 100)]));
    vi.mocked(api.diff).mockClear();
    args.pane = { kind: "diff", value: { ...diff, path: "b.ts" } };
    hook.rerender();
    await send(status([row("a.ts", 100), row("b.ts", 200)]));
    expect(api.diff).toHaveBeenCalledTimes(1);
    expect(api.diff).toHaveBeenCalledWith("repo-1", "b.ts");
  });

  it("lets the latest refresh land when an event repeats while one is in flight", async () => {
    const resolvers: ((value: Diff) => void)[] = [];
    const { send, setPane } = setup();
    vi.mocked(api.diff).mockResolvedValue(diff);
    await send(status([row("a.ts", 100)]));
    vi.mocked(api.diff).mockClear();
    setPane.mockClear();
    vi.mocked(api.diff).mockImplementation(
      () =>
        new Promise<Diff>((done) => {
          resolvers.push(done);
        }),
    );
    await send(status([row("a.ts", 200)]));
    await send(status([row("b.ts", 200), row("a.ts", 200)]));
    expect(api.diff).toHaveBeenCalledTimes(2);
    await act(async () => resolvers.forEach((resolve) => resolve(diff)));
    expect(setPane).toHaveBeenCalledTimes(1);
    expect(setPane).toHaveBeenCalledWith(
      expect.objectContaining({ kind: "diff", value: diff }),
    );
  });
});
