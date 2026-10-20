// @vitest-environment happy-dom

import { act, renderHook } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { usePaneLoading } from "./usePaneLoading";

function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (error: unknown) => void;
  const promise = new Promise<T>((res, rej) => {
    resolve = res;
    reject = rej;
  });
  return { promise, resolve, reject };
}

describe("usePaneLoading", () => {
  it("요청이_끝나면_내려간다", async () => {
    const { result } = renderHook(() => usePaneLoading());
    const fetch = deferred<string>();
    act(() => void result.current.track(1, fetch.promise));
    expect(result.current.loading).toBe(true);
    await act(async () => fetch.resolve("ok"));
    expect(result.current.loading).toBe(false);
  });

  it("실패해도_내려간다", async () => {
    const { result } = renderHook(() => usePaneLoading());
    const fetch = deferred<string>();
    act(() => void result.current.track(1, fetch.promise).catch(() => undefined));
    await act(async () => fetch.reject(new Error("boom")));
    expect(result.current.loading).toBe(false);
  });

  it("늦게_끝난_이전_요청은_새_요청의_표시를_지우지_않는다", async () => {
    const { result } = renderHook(() => usePaneLoading());
    const older = deferred<string>();
    const newer = deferred<string>();
    act(() => void result.current.track(1, older.promise));
    act(() => void result.current.track(2, newer.promise));
    await act(async () => older.resolve("stale"));
    expect(result.current.loading).toBe(true);
    await act(async () => newer.resolve("fresh"));
    expect(result.current.loading).toBe(false);
  });
});
