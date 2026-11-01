// @vitest-environment happy-dom

import { cleanup, render } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { ChangedFile, Commit } from "../../api";
import { LogList, type LogListProps } from "./LogList";

afterEach(cleanup);

function commit(index: number): Commit {
  return {
    oid: `oid-${index}`,
    short_id: `oid-${index}`,
    summary: `commit ${index}`,
    author: "a",
    time: 0,
  };
}

function changed(index: number): ChangedFile {
  return { path: `f/${index}.ts`, index: "M", worktree: " " };
}

const decorations = {
  refsOf: () => undefined,
  divergenceOf: () => undefined,
  truncated: false,
};

function props(overrides: Partial<LogListProps> = {}): LogListProps {
  return {
    visibleCommits: [],
    decorations,
    commits: [],
    commitDrillDown: null,
    visibleCommitFiles: [],
    logDone: true,
    logStalled: false,
    logPagingPaused: false,
    viewport: { scrollTop: 0, height: 400 },
    setLogStalled: () => {},
    logSentinelRef: { current: null },
    openCommitFiles: () => {},
    openCommit: () => {},
    openCommitFileDiff: () => {},
    setCommitDrillDown: () => {},
    setPaneEmpty: () => {},
    bumpPaneRequest: () => {},
    ...overrides,
  };
}

describe("LogList virtualization", () => {
  it("a long history renders only the viewport and its overscan", () => {
    const commits = Array.from({ length: 2_000 }, (_, i) => commit(i));
    const view = render(
      <ul>
        <LogList
          {...props({
            visibleCommits: commits,
            commits,
            viewport: { scrollTop: 20_000, height: 400 },
          })}
        />
      </ul>,
    );
    expect(
      view.container.querySelector("[data-virtual-count]")?.getAttribute("data-virtual-count"),
    ).toBe("2000");
    expect(view.container.querySelectorAll("[data-virtual-row]").length).toBeLessThan(50);
    expect(view.container.querySelector("[data-virtual-row='1000']")).not.toBeNull();
    expect(view.container.querySelector("[data-virtual-row='0']")).toBeNull();
  });

  it("keeps the paging sentinel at the true end of a virtualized history", () => {
    const commits = Array.from({ length: 2_000 }, (_, i) => commit(i));
    const view = render(
      <ul>
        <LogList
          {...props({
            visibleCommits: commits,
            commits,
            logDone: false,
            viewport: { scrollTop: 20_000, height: 400 },
          })}
        />
      </ul>,
    );
    expect(view.container.querySelectorAll("[data-virtual-row]").length).toBeLessThan(50);
    // The skeleton that asks for the next page is still mounted, even though
    // the rows that far down are not.
    expect(view.container.querySelector(".nc-skeleton")).not.toBeNull();
  });

  it("a long commit file list renders only its window too", () => {
    const files = Array.from({ length: 2_000 }, (_, i) => changed(i));
    const view = render(
      <ul>
        <LogList
          {...props({
            commitDrillDown: { commit: commit(0), files, truncated: false },
            commits: [commit(0)],
            visibleCommitFiles: files,
            viewport: { scrollTop: 20_000, height: 400 },
          })}
        />
      </ul>,
    );
    expect(
      view.container.querySelector("[data-virtual-count]")?.getAttribute("data-virtual-count"),
    ).toBe("2000");
    expect(view.container.querySelector("[data-virtual-row='1000']")).not.toBeNull();
    expect(view.container.querySelector("[data-virtual-row='0']")).toBeNull();
    expect(view.getByText("all changes")).toBeTruthy();
  });

  it("a small history keeps every row in the DOM", () => {
    const commits = Array.from({ length: 3 }, (_, i) => commit(i));
    const view = render(
      <ul>
        <LogList {...props({ visibleCommits: commits, commits })} />
      </ul>,
    );
    expect(view.container.querySelector("[data-virtual-count]")).toBeNull();
    expect(view.container.querySelectorAll("li")).toHaveLength(3);
  });

  it("opens the commit under the window", () => {
    const commits = Array.from({ length: 2_000 }, (_, i) => commit(i));
    const opened = vi.fn();
    const view = render(
      <ul>
        <LogList
          {...props({
            visibleCommits: commits,
            commits,
            openCommitFiles: opened,
            viewport: { scrollTop: 20_000, height: 400 },
          })}
        />
      </ul>,
    );
    view.container
      .querySelector<HTMLElement>("[data-virtual-row='1000']")
      ?.querySelector("button")
      ?.click();
    expect(opened).toHaveBeenCalledWith(expect.objectContaining({ oid: "oid-1000" }));
  });
});
