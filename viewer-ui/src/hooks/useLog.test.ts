// @vitest-environment happy-dom

import { act, cleanup, renderHook } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { useLog } from "./useLog";
import type { Commit, Log } from "../api";

vi.mock("../api", () => ({
  api: { log: vi.fn() },
}));

// The mock above replaces the module, so this import resolves to it.
import { api } from "../api";
const apiLog = vi.mocked(api.log);

function commit(oid: string): Commit {
  return { oid, short_id: oid.slice(0, 7), summary: oid, author: "a", time: 0 };
}

function page(...oids: string[]): Log {
  return { commits: oids.map(commit), truncated: true, head: oids[0] };
}

// Stable like the real one (`useAppViewModel` memoizes it): a fresh identity
// per render would re-run the refresh effect on every render, which is not
// the wiring these tests are about.
const handle = () => {};

function render(head: string | null | undefined) {
  return renderHook(
    (props: { head: string | null | undefined }) =>
      useLog({
        repo: "r1",
        authed: true,
        tab: "log",
        filter: "",
        head: props.head,
        handle,
      }),
    { initialProps: { head } },
  );
}

/** Let the fetch the hook fired on render resolve and land. */
const settle = () => act(async () => {});

beforeEach(() => {
  apiLog.mockReset();
});

// Vitest runs without globals, so RTL cannot auto-register its cleanup — done
// here so one test's mounted hooks and listeners do not leak into the next.
afterEach(cleanup);

describe("useLog on a head move", () => {
  it("does not refetch on the first head it observes", async () => {
    apiLog.mockResolvedValueOnce(page("b", "c"));
    const { result } = render("b");
    await settle();

    // Only the initial page load — the first head is a baseline, not a move.
    expect(apiLog).toHaveBeenCalledTimes(1);
    expect(result.current.commits.map((c) => c.oid)).toEqual(["b", "c"]);
  });

  it("prepends the new commits when the head advances", async () => {
    apiLog.mockResolvedValueOnce(page("b", "c"));
    const { result, rerender } = render("b");
    await settle();

    apiLog.mockResolvedValueOnce(page("a", "b"));
    rerender({ head: "a" });
    await settle();

    expect(apiLog).toHaveBeenCalledTimes(2);
    // The refresh asks for a fresh first page, not a continuation.
    expect(apiLog).toHaveBeenLastCalledWith("r1");
    expect(result.current.commits.map((c) => c.oid)).toEqual(["a", "b", "c"]);
  });

  it("replaces the list when the history diverged", async () => {
    apiLog.mockResolvedValueOnce(page("b", "c"));
    const { result, rerender } = render("b");
    await settle();

    // A rebase: the old head is gone from the fresh page.
    apiLog.mockResolvedValueOnce(page("x", "y"));
    rerender({ head: "x" });
    await settle();

    expect(result.current.commits.map((c) => c.oid)).toEqual(["x", "y"]);
  });

  it("a status stream going quiet is not a move", async () => {
    apiLog.mockResolvedValueOnce(page("b", "c"));
    const { rerender } = render("b");
    await settle();

    // `undefined` is the stream not reporting — a reconnect, a project just
    // opened — and silence resolving to the same head is no change at all.
    rerender({ head: undefined });
    rerender({ head: "b" });
    await settle();

    expect(apiLog).toHaveBeenCalledTimes(1);
  });

  it("an empty repository's first commit fills the list", async () => {
    // The empty history is a loaded state, not a missing baseline: the head
    // appearing over it is a disagreement and must refresh.
    apiLog.mockResolvedValueOnce({ commits: [], truncated: false });
    const { result, rerender } = render(null);
    await settle();
    expect(result.current.logDone).toBe(true);

    apiLog.mockResolvedValueOnce({
      commits: [commit("a")],
      truncated: false,
      head: "a",
    });
    rerender({ head: "a" });
    await settle();

    expect(result.current.commits.map((c) => c.oid)).toEqual(["a"]);
  });

  it("a branch going unborn empties the list", async () => {
    // `null` is a report like any oid: the server saw the repository and it
    // has no head (an orphan checkout), so a list of commits is stale.
    apiLog.mockResolvedValueOnce(page("b", "c"));
    const { result, rerender } = render("b");
    await settle();

    apiLog.mockResolvedValueOnce({ commits: [], truncated: false });
    rerender({ head: null });
    await settle();

    expect(result.current.commits).toEqual([]);
    expect(result.current.logDone).toBe(true);
  });

  it("a first page the head moved past is refreshed on landing", async () => {
    // The status stream can overtake a first page in flight; its report is
    // newer than the walk the page came from.
    let land!: (page: Log) => void;
    apiLog.mockImplementationOnce(
      () => new Promise<Log>((resolve) => (land = resolve)),
    );
    const { result, rerender } = render("b");
    rerender({ head: "a" });

    apiLog.mockResolvedValueOnce(page("a", "b"));
    await act(async () => land(page("b", "c")));
    await settle();

    expect(result.current.commits.map((c) => c.oid)).toEqual(["a", "b", "c"]);
  });

  it("a walk newer than the report is not asked again", async () => {
    // The status stream lags the repository: the refresh asked for "a" can
    // return history already at "c". That leaves a standing disagreement no
    // further fetch resolves — one ask per reported head, not a loop.
    apiLog.mockResolvedValueOnce(page("b", "c"));
    const { result, rerender } = render("b");
    await settle();

    apiLog.mockResolvedValue(page("z", "a", "b"));
    rerender({ head: "a" });
    await settle();
    await settle();

    expect(apiLog).toHaveBeenCalledTimes(2);
    expect(result.current.commits.map((c) => c.oid)).toEqual([
      "z",
      "a",
      "b",
      "c",
    ]);

    // The stream catching up to the walk is agreement, not another move.
    rerender({ head: "z" });
    await settle();
    expect(apiLog).toHaveBeenCalledTimes(2);
  });

  it("a reset back to an already-asked head still refreshes", async () => {
    // The ask for "a" was answered with newer history, and the stream then
    // caught up with it — which spends the ask, so a later genuine reset back
    // to "a" is a move, not the old race replaying.
    apiLog.mockResolvedValueOnce(page("b", "c"));
    const { result, rerender } = render("b");
    await settle();

    apiLog.mockResolvedValueOnce(page("z", "a", "b"));
    rerender({ head: "a" });
    await settle();

    rerender({ head: "z" });
    await settle();
    expect(apiLog).toHaveBeenCalledTimes(2);

    apiLog.mockResolvedValueOnce(page("a", "b"));
    rerender({ head: "a" });
    await settle();

    expect(apiLog).toHaveBeenCalledTimes(3);
    expect(result.current.commits.map((c) => c.oid)).toEqual(["a", "b"]);
  });

  it("an ask in flight survives a passing agreement", async () => {
    // The head flapping back to the cache's value while a refresh is pending
    // agrees with the cache that refresh is about to replace; spending the
    // ask on it would drop the loop guard for the answer about to land.
    apiLog.mockResolvedValueOnce(page("b", "c"));
    const { result, rerender } = render("b");
    await settle();

    let land!: (page: Log) => void;
    apiLog.mockImplementationOnce(
      () => new Promise<Log>((resolve) => (land = resolve)),
    );
    rerender({ head: "a" });
    rerender({ head: "b" });
    await act(async () => land(page("z", "a", "b")));

    // The mark still guards "a": the walk outran the report, and asking again
    // would only fetch the same history.
    rerender({ head: "a" });
    await settle();
    expect(apiLog).toHaveBeenCalledTimes(2);

    // Agreement at rest spends it, and a genuine reset back refreshes.
    rerender({ head: "z" });
    await settle();
    apiLog.mockResolvedValueOnce(page("a", "b"));
    rerender({ head: "a" });
    await settle();

    expect(apiLog).toHaveBeenCalledTimes(3);
    expect(result.current.commits.map((c) => c.oid)).toEqual(["a", "b"]);
  });

  it("retrying a failed refresh refreshes again", async () => {
    apiLog.mockResolvedValueOnce(page("b", "c"));
    const { result, rerender } = render("b");
    await settle();

    apiLog.mockRejectedValueOnce(new Error("net down"));
    rerender({ head: "a" });
    await settle();
    expect(result.current.logStalled).toBe(true);
    expect(result.current.commits.map((c) => c.oid)).toEqual(["b", "c"]);

    // The retry row only clears the stall; the standing disagreement between
    // the cache and the head is what turns that into another refresh.
    apiLog.mockResolvedValueOnce(page("a", "b"));
    act(() => result.current.setLogStalled(false));
    await settle();

    expect(result.current.commits.map((c) => c.oid)).toEqual(["a", "b", "c"]);
    expect(result.current.logStalled).toBe(false);
  });
});
