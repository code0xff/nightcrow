// @vitest-environment happy-dom
//
// The log's decorations are repository-wide and replaced wholesale, asked
// again whenever HEAD, the branch, or any ref moves — and again after a
// failure, so a change is never dropped on the floor.

import { act, cleanup, renderHook } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  useLogDecorations,
  type UseLogDecorationsArgs,
} from "./useLogDecorations";
import type { LogDecorations } from "../../api";

vi.mock("../../api", () => ({
  api: { logDecorations: vi.fn() },
}));

import { api } from "../../api";
const fetchMarks = vi.mocked(api.logDecorations);

function marks(over: Partial<LogDecorations> = {}): LogDecorations {
  return { refs: {}, ahead: [], behind: [], truncated: false, ...over };
}

type Props = Pick<UseLogDecorationsArgs, "head" | "branch" | "refs"> & {
  upstream?: string;
};

function render(initial: Props) {
  return renderHook(
    (props: Props) =>
      useLogDecorations({
        repo: "r1",
        authed: true,
        tab: "log",
        upstream: undefined,
        ...props,
      }),
    { initialProps: initial },
  );
}

const settle = () => act(async () => {});

beforeEach(() => {
  fetchMarks.mockReset();
  vi.useRealTimers();
});
afterEach(cleanup);

describe("useLogDecorations", () => {
  it("refreshes every row decoration when only a ref changes", async () => {
    fetchMarks.mockResolvedValueOnce(marks({ ahead: ["a", "z"] }));
    const { result, rerender } = render({
      head: "a",
      branch: "dev",
      refs: "r1",
    });
    await settle();
    // `z` stands for a row far below the first page: it is answered too.
    expect(result.current.divergenceOf("z")).toBe("ahead");

    // Pushed: HEAD did not move, the upstream caught up.
    fetchMarks.mockResolvedValueOnce(marks());
    rerender({ head: "a", branch: "dev", refs: "r2" });
    await settle();

    expect(fetchMarks).toHaveBeenCalledTimes(2);
    expect(result.current.divergenceOf("a")).toBeUndefined();
    expect(result.current.divergenceOf("z")).toBeUndefined();
  });

  it("queries again when only the branch changes at the same commit", async () => {
    // Neither HEAD nor the refs digest moves; only the branch HEAD is on.
    fetchMarks.mockResolvedValueOnce(
      marks({ refs: { a: [{ kind: "head", name: "dev" }] } }),
    );
    const { result, rerender } = render({
      head: "a",
      branch: "dev",
      refs: "r1",
    });
    await settle();

    fetchMarks.mockResolvedValueOnce(
      marks({ refs: { a: [{ kind: "head", name: "feat" }] } }),
    );
    rerender({ head: "a", branch: "feat", refs: "r1" });
    await settle();

    expect(result.current.refsOf("a")?.[0]?.name).toBe("feat");
  });

  it("queries again when only the upstream changes", async () => {
    // `git branch --set-upstream-to`: no ref target moves, the arrows do.
    fetchMarks.mockResolvedValueOnce(marks({ ahead: ["a"] }));
    const { result, rerender } = render({
      head: "a",
      branch: "dev",
      refs: "r1",
      upstream: "origin/dev",
    });
    await settle();

    fetchMarks.mockResolvedValueOnce(marks());
    rerender({ head: "a", branch: "dev", refs: "r1", upstream: "fork/dev" });
    await settle();

    expect(fetchMarks).toHaveBeenCalledTimes(2);
    expect(result.current.divergenceOf("a")).toBeUndefined();
  });

  it("retries a failed change instead of discarding it", async () => {
    vi.useFakeTimers();
    fetchMarks.mockRejectedValueOnce(new Error("offline"));
    const { result } = render({ head: "a", branch: "dev", refs: "r1" });
    await act(async () => {});

    fetchMarks.mockResolvedValueOnce(marks({ ahead: ["a"] }));
    await act(async () => {
      await vi.advanceTimersByTimeAsync(5000);
    });

    expect(fetchMarks).toHaveBeenCalledTimes(2);
    expect(result.current.divergenceOf("a")).toBe("ahead");
  });

  it("does not query again when nothing changes", async () => {
    fetchMarks.mockResolvedValue(marks());
    const { rerender } = render({ head: "a", branch: "dev", refs: "r1" });
    await settle();

    rerender({ head: "a", branch: "dev", refs: "r1" });
    await settle();

    expect(fetchMarks).toHaveBeenCalledTimes(1);
  });

  it("reports when a response is truncated", async () => {
    fetchMarks.mockResolvedValueOnce(marks({ truncated: true }));
    const { result } = render({ head: "a", branch: "dev", refs: "r1" });
    await settle();

    expect(result.current.truncated).toBe(true);
  });

  it("does not query before status arrives", async () => {
    render({ head: undefined, branch: undefined, refs: undefined });
    await settle();

    expect(fetchMarks).not.toHaveBeenCalled();
  });
});
