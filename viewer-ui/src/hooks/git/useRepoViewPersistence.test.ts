// @vitest-environment happy-dom

import { act, cleanup, renderHook } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { RepoView } from "../../api";
import { blankView, workdirFile } from "../../lib/repository/repoView";
import type { UsePaneOpenersResult } from "./usePaneOpeners";
import { useRepoViewPersistence } from "./useRepoViewPersistence";

afterEach(cleanup);

function harness(known: boolean, remembered?: RepoView) {
  const store: Record<string, RepoView> = remembered ? { r1: remembered } : {};
  const remember = vi.fn((repo: string | null, view: RepoView) => {
    if (repo) store[repo] = view;
  });
  const openers: UsePaneOpenersResult = {
    openDiff: vi.fn(),
    openFile: vi.fn(),
    openCommit: vi.fn(),
    openCommitFileDiff: vi.fn(),
    openCommitFiles: vi.fn(async () => undefined),
    showOtherFace: vi.fn(),
  };
  const setTab = vi.fn();
  const clearPane = vi.fn();
  const rendered = renderHook(
    (props: { known: boolean; remembered?: RepoView }) =>
      useRepoViewPersistence({
        repo: "r1",
        known: props.known,
        remembered: props.remembered,
        latest: (repo) => store[repo],
        remember,
        setTab,
        clearPane,
        openers,
      }),
    { initialProps: { known, remembered } },
  );
  return { ...rendered, remember, openers, setTab, clearPane, store };
}

describe("useRepoViewPersistence grouped contract", () => {
  it("restoration calls the opener but does not write again", () => {
    const remembered = {
      ...blankView(),
      tab: "tree" as const,
      file: workdirFile("src/a.ts", "diff"),
    };
    const h = harness(true, remembered);

    expect(h.setTab).toHaveBeenCalledWith("tree");
    expect(h.openers.openDiff).toHaveBeenCalledWith("src/a.ts", {
      restoring: true,
    });
    expect(h.remember).not.toHaveBeenCalled();
  });

  it("preserves a selection made before the restore response and merges it over the old view", () => {
    const old = {
      ...blankView(),
      tab: "tree" as const,
      tree_expanded: ["src"],
    };
    const h = harness(false);
    act(() => h.result.current.asked.openFile("README.md"));
    h.store.r1 = old;
    h.rerender({ known: true, remembered: old });

    expect(h.openers.openFile).toHaveBeenCalledWith("README.md");
    expect(h.openers.openFile).not.toHaveBeenCalledWith(expect.anything(), {
      restoring: true,
    });
    expect(h.remember).toHaveBeenLastCalledWith("r1", {
      ...old,
      file: workdirFile("README.md", "source"),
    });
  });

  it("sends opener, tab, drilldown, and tree selection through one write boundary", async () => {
    const h = harness(true, blankView());
    act(() => h.result.current.chooseTab("log"));
    act(() => h.result.current.asked.openDiff("a.ts"));
    act(() => h.result.current.asked.openCommit("deadbeef"));
    act(() => h.result.current.noteTree(["src", "src/lib"]));
    act(() => h.result.current.forgetPane());

    expect(h.setTab).toHaveBeenCalledWith("log");
    expect(h.openers.openDiff).toHaveBeenCalledWith("a.ts");
    expect(h.openers.openCommit).toHaveBeenCalledWith("deadbeef");
    expect(h.clearPane).toHaveBeenCalled();
    expect(h.store.r1).toEqual({
      tab: "log",
      file: null,
      tree_expanded: ["src", "src/lib"],
    });
  });
});
