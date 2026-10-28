// @vitest-environment happy-dom
//
// The regression net for the view-memory review rounds: every defect they shed
// lived in this hook's ordering — restore vs record, project switches, the
// window before the server's response. Each scenario here is one of them.

import { act, cleanup, renderHook } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { useRepoViewMemory } from "./useRepoViewMemory";
import { blankView, workdirFile } from "../../lib/repository/repoView";
import type { RepoView, RepoViewByRepo } from "../../api";

interface HarnessProps {
  repo: string | null;
  known: boolean;
  remembered: RepoView | undefined;
}

/** The hook with its collaborators mocked, plus a store `remember` writes and
 *  `latest` reads — the shape `useRepoView` gives the real thing. `seed` plays
 *  the poll's `adopt`, which in the real wiring has already put the server's
 *  view into that store by the time `remembered` names it. */
function harness(initial: HarnessProps, seed: RepoViewByRepo = {}) {
  const store: RepoViewByRepo = { ...seed };
  const remember = vi.fn((repo: string | null, view: RepoView) => {
    if (repo != null) store[repo] = view;
  });
  const setTab = vi.fn();
  const openDiff = vi.fn();
  const openFile = vi.fn();
  const openCommitFileDiff = vi.fn();
  const rendered = renderHook(
    (props: HarnessProps) =>
      useRepoViewMemory({
        ...props,
        latest: (repo) => store[repo],
        remember,
        setTab,
        openDiff,
        openFile,
        openCommitFileDiff,
      }),
    { initialProps: initial },
  );
  return {
    ...rendered,
    store,
    remember,
    setTab,
    openDiff,
    openFile,
    openCommitFileDiff,
  };
}

// Vitest runs without globals, so RTL cannot auto-register its cleanup.
afterEach(cleanup);

const memoryOf = (path: string): RepoView => ({
  ...blankView(),
  tab: "tree",
  file: workdirFile(path, "diff"),
});

describe("Restoration", () => {
  it("opening a project restores its remembered tab and file once", () => {
    const h = harness({ repo: "r1", known: true, remembered: memoryOf("a.ts") });

    expect(h.setTab).toHaveBeenCalledWith("tree");
    expect(h.openDiff).toHaveBeenCalledWith("a.ts", { restoring: true });

    // A rerender is not a reason to reopen; that would restore the file just closed.
    h.rerender({ repo: "r1", known: true, remembered: memoryOf("a.ts") });
    expect(h.openDiff).toHaveBeenCalledTimes(1);
  });

  it("restores again after leaving and returning", () => {
    // Switching projects cleared the pane, so returning is equivalent to opening the project.
    const h = harness({ repo: "r1", known: true, remembered: memoryOf("a.ts") });
    h.rerender({ repo: "r2", known: true, remembered: undefined });
    h.rerender({ repo: "r1", known: true, remembered: memoryOf("a.ts") });

    expect(h.openDiff).toHaveBeenCalledTimes(2);
  });

  it("waits for the response before restoring, then restores", () => {
    // The map cannot distinguish “no remembered view” from “not received yet”; known separates them.
    const h = harness({ repo: "r1", known: false, remembered: undefined });
    expect(h.setTab).not.toHaveBeenCalled();

    h.rerender({ repo: "r1", known: true, remembered: memoryOf("a.ts") });
    expect(h.openDiff).toHaveBeenCalledWith("a.ts", { restoring: true });
  });

  it("falls back to the status tab when no view is remembered", () => {
    // Otherwise a project without its own view inherits the previous project’s tab.
    const h = harness({ repo: "r1", known: true, remembered: undefined });
    expect(h.setTab).toHaveBeenCalledWith("status");
    expect(h.openDiff).not.toHaveBeenCalled();
  });

  it("restores the next project even after passing through one whose response has not arrived", () => {
    // A → (just opened) B → A: failing to restore B must not block restoring A.
    const h = harness({ repo: "r1", known: true, remembered: memoryOf("a.ts") });
    h.rerender({ repo: "r2", known: false, remembered: undefined });
    h.rerender({ repo: "r1", known: true, remembered: memoryOf("a.ts") });

    expect(h.openDiff).toHaveBeenCalledTimes(2);
  });

  it("calls the opener for the remembered view", () => {
    // This pins down which opener the hook wires to the answer, not just a branch of the pure restoreOpen function.

    const source = harness({
      repo: "r1",
      known: true,
      remembered: { ...blankView(), file: workdirFile("a.ts", "source") },
    });
    expect(source.openFile).toHaveBeenCalledWith("a.ts", { restoring: true });
    expect(source.openDiff).not.toHaveBeenCalled();

    const fromCommit = harness({
      repo: "r1",
      known: true,
      remembered: {
        ...blankView(),
        file: { path: "b.ts", commit: "9a3bc2c", face: "source" },
      },
    });
    expect(fromCommit.openCommitFileDiff).toHaveBeenCalledWith("9a3bc2c", "b.ts", {
      restoring: true,
    });
  });

  it("selecting in A and switching to B restores B from its own memory", () => {
    // touched belongs to a visit: a selection in A must not block restoring B.
    const h = harness({ repo: "r1", known: true, remembered: undefined });
    act(() => h.result.current.noteFile(workdirFile("mine.ts", "diff")));

    h.rerender({ repo: "r2", known: true, remembered: memoryOf("b.ts") });

    expect(h.openDiff).toHaveBeenCalledWith("b.ts", { restoring: true });
    expect(h.result.current.touched).toBe(false);
  });

  it("restoration does not write anything", () => {
    // This restores saved state. Writing here could let a failure erase the remembered view.
    const h = harness({ repo: "r1", known: true, remembered: memoryOf("a.ts") });
    expect(h.remember).not.toHaveBeenCalled();
  });
});

describe("Recording", () => {
  it("merges a selection into the current value so two selections in one tick do not erase each other", () => {
    // A tab switch changes both the tab and the pane it clears; merging each into a render copy
    // would let the latter erase the former.
    const h = harness({ repo: "r1", known: true, remembered: undefined });

    act(() => {
      h.result.current.noteTab("log");
      h.result.current.noteFile(null);
    });

    expect(h.store.r1).toEqual({ ...blankView(), tab: "log", file: null });
  });

  it("selecting a file does not erase other remembered items", () => {
    const h = harness(
      { repo: "r1", known: true, remembered: memoryOf("a.ts") },
      { r1: memoryOf("a.ts") },
    );

    act(() => h.result.current.noteFile(workdirFile("b.ts", "source")));

    // The tree and tab stay the same; only the file changes.
    expect(h.store.r1.tab).toBe("tree");
    expect(h.store.r1.file).toEqual(workdirFile("b.ts", "source"));
  });
});

describe("Selection before the response", () => {
  it("is retained and merged over the response; restoration does not override it", () => {
    const h = harness({ repo: "r1", known: false, remembered: undefined });

    act(() => h.result.current.noteFile(workdirFile("mine.ts", "diff")));
    expect(h.remember).not.toHaveBeenCalled();

    h.rerender({ repo: "r1", known: true, remembered: memoryOf("old.ts") });

    // The selected file overlays the remembered view, while the unselected tree keeps its previous value.
    expect(h.store.r1.file).toEqual(workdirFile("mine.ts", "diff"));
    expect(h.store.r1.tab).toBe("tree");
    // The screen is already in use, so restoration must not overwrite it.
    expect(h.openDiff).not.toHaveBeenCalled();
    expect(h.result.current.touched).toBe(true);
  });

  it("selection stays with the project while waiting and is applied on return", () => {
    // Leaving does not mean the user changed their mind.
    const h = harness({ repo: "r1", known: false, remembered: undefined });
    act(() => h.result.current.noteTab("log"));

    h.rerender({ repo: "r2", known: true, remembered: undefined });
    h.rerender({ repo: "r1", known: true, remembered: undefined });

    expect(h.store.r1.tab).toBe("log");
  });

  it("a pending selection for one project does not leak into another", () => {
    const h = harness({ repo: "r1", known: false, remembered: undefined });
    act(() => h.result.current.noteTab("log"));

    // r2 responds first; the selection for r1 must not affect r2.
    h.rerender({ repo: "r2", known: true, remembered: undefined });
    expect(h.store.r2).toBeUndefined();
  });
});
