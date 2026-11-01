// @vitest-environment happy-dom

import { act, cleanup, render } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { Commit, Log } from "../../api";
import { useLog, type UseLogResult } from "./useLog";

vi.mock("../../api", () => ({
  api: { log: vi.fn() },
}));

import { api } from "../../api";

const apiLog = vi.mocked(api.log);

function commit(oid: string): Commit {
  return { oid, short_id: oid, summary: oid, author: "a", time: 0 };
}

function page(oids: string[], truncated = true): Log {
  return {
    commits: oids.map(commit),
    truncated,
    head: oids[0],
  };
}

class MockObserver {
  static latest: MockObserver | null = null;
  readonly callback: IntersectionObserverCallback;
  constructor(callback: IntersectionObserverCallback) {
    this.callback = callback;
    MockObserver.latest = this;
  }
  observe() {}
  unobserve() {}
  disconnect() {}
  takeRecords(): IntersectionObserverEntry[] {
    return [];
  }
  trigger() {
    this.callback(
      [{ isIntersecting: true } as IntersectionObserverEntry],
      this as unknown as IntersectionObserver,
    );
  }
}

let log: UseLogResult;

function Harness({ head }: { head: string | null | undefined }) {
  log = useLog({
    repo: "r1",
    authed: true,
    tab: "log",
    filter: "",
    head,
    handle: () => {},
  });
  return (
    <ul>
      <li ref={log.logSentinelRef} />
    </ul>
  );
}

const settle = () => act(async () => {});

beforeEach(() => {
  apiLog.mockReset();
  MockObserver.latest = null;
  vi.stubGlobal("IntersectionObserver", MockObserver);
});

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});

describe("useLog paging", () => {
  it("appends a page into the list instead of copying the history", async () => {
    apiLog.mockResolvedValueOnce(page(["c", "d"]));
    render(<Harness head="c" />);
    await settle();
    const held = log.commits;
    expect(held.map((c) => c.oid)).toEqual(["c", "d"]);

    apiLog.mockResolvedValueOnce(page(["a", "b"], false));
    act(() => MockObserver.latest?.trigger());
    await settle();

    // The continuation carries the same anchor and skips what is already held,
    // and its rows join the array in place: no copy of the history per page.
    expect(apiLog).toHaveBeenLastCalledWith("r1", { from: "c", skip: 2 });
    expect(log.commits).toBe(held);
    expect(log.commits.map((c) => c.oid)).toEqual(["c", "d", "a", "b"]);
    expect(log.logDone).toBe(true);
  });

  it("a reset gives the new list its own identity", async () => {
    apiLog.mockResolvedValueOnce(page(["c", "d"]));
    render(<Harness head="c" />);
    await settle();
    const held = log.commits;

    act(() => log.resetLog());
    await settle();
    expect(log.commits).not.toBe(held);
    expect(log.commits).toEqual([]);
  });
});
