import { describe, expect, it } from "vitest";
import { reconcileLog } from "./logRefresh";
import type { Commit, Log } from "../api";

function commit(oid: string): Commit {
  return { oid, short_id: oid.slice(0, 7), summary: oid, author: "a", time: 0 };
}

function commits(...oids: string[]): Commit[] {
  return oids.map(commit);
}

function page(oids: string[], opts?: { truncated?: boolean }): Log {
  return {
    commits: commits(...oids),
    truncated: opts?.truncated ?? true,
    head: oids[0],
  };
}

describe("reconcileLog", () => {
  it("prepends only the new commits on a fast-forward", () => {
    const cached = commits("c", "d", "e");
    const out = reconcileLog(page(["a", "b", "c", "d"]), cached, false);

    expect(out.mode).toBe("prepend");
    expect(out.commits.map((c) => c.oid)).toEqual(["a", "b", "c", "d", "e"]);
    expect(out.anchor).toBe("a");
    // The fresh page was truncated, and the cached tail was not complete.
    expect(out.done).toBe(false);
  });

  it("keeps a finished tail finished across a prepend", () => {
    const out = reconcileLog(page(["a", "b"]), commits("b", "c"), true);

    expect(out.mode).toBe("prepend");
    expect(out.done).toBe(true);
  });

  it("finishes when an untruncated fresh page covers the whole cache", () => {
    // Untruncated means the fresh page is the entire history, and a prepend
    // only matches when the cache held everything below the old head.
    const out = reconcileLog(
      page(["a", "b", "c"], { truncated: false }),
      commits("b", "c"),
      false,
    );

    expect(out.mode).toBe("prepend");
    expect(out.commits.map((c) => c.oid)).toEqual(["a", "b", "c"]);
    expect(out.done).toBe(true);
  });

  it("an unmoved head prepends nothing", () => {
    const cached = commits("a", "b", "c");
    const out = reconcileLog(page(["a", "b"]), cached, false);

    expect(out.mode).toBe("prepend");
    // The very same array: a refresh that found no movement must set the
    // state it read, so React can bail on the render.
    expect(out.commits).toBe(cached);
  });

  it("replaces when the old head is gone from the fresh page", () => {
    // A rebase rewrote the history the cache came from.
    const out = reconcileLog(page(["x", "y"]), commits("a", "b"), true);

    expect(out.mode).toBe("replace");
    expect(out.commits.map((c) => c.oid)).toEqual(["x", "y"]);
    expect(out.anchor).toBe("x");
    expect(out.done).toBe(false);
  });

  it("replaces when the entries under the old head diverge", () => {
    // A merge can interleave side-branch commits below the old head; the
    // cached pages are then not a contiguous run of the new history.
    const out = reconcileLog(
      page(["m", "a", "s", "b"]),
      commits("a", "b", "c"),
      false,
    );

    expect(out.mode).toBe("replace");
    expect(out.commits.map((c) => c.oid)).toEqual(["m", "a", "s", "b"]);
  });

  it("trusts the entries below the fresh page boundary", () => {
    // The check can only see the fresh page: a merge whose side-branch
    // commits date-sort below it still prepends. The accepted bet the module
    // doc describes, shared with the TUI's rule.
    const out = reconcileLog(page(["a", "b"]), commits("b", "c", "d"), false);

    expect(out.mode).toBe("prepend");
    expect(out.commits.map((c) => c.oid)).toEqual(["a", "b", "c", "d"]);
  });

  it("replaces when the fresh tail outruns the cached pages", () => {
    // Everything held matches, but the fresh page continues past it — the
    // combined list would skip the unseen entries, so it cannot be kept.
    const out = reconcileLog(page(["n", "a", "b", "c"]), commits("a", "b"), false);

    expect(out.mode).toBe("replace");
  });

  it("an empty cache takes the fresh page as the first one", () => {
    const out = reconcileLog(page(["a", "b"]), [], false);

    expect(out.mode).toBe("replace");
    expect(out.commits.map((c) => c.oid)).toEqual(["a", "b"]);
  });

  it("an anchorless page marks the history done", () => {
    // Only a repository with no commits to anchor to omits the head.
    const out = reconcileLog(
      { commits: [], truncated: false },
      commits("a"),
      false,
    );

    expect(out.mode).toBe("replace");
    expect(out.commits).toEqual([]);
    expect(out.anchor).toBeNull();
    expect(out.done).toBe(true);
  });
});
