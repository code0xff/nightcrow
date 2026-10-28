// @vitest-environment happy-dom

import { act, cleanup, renderHook } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { useRepoView } from "./useRepoView";
import { useRepoViewMemory } from "./useRepoViewMemory";
import { blankView, workdirFile } from "../../lib/repository/repoView";
import type { RepoView } from "../../api";

vi.mock("../../api", () => ({
  api: { setRepoView: vi.fn(() => Promise.resolve({})) },
}));

// The mock above replaces the module, so this import resolves to it.
import { api } from "../../api";
const setRepoView = vi.mocked(api.setRepoView);

const aView: RepoView = { ...blankView(), file: workdirFile("a.ts", "diff") };
const bView: RepoView = { ...blankView(), tab: "log" };

/** Let the serial writer's in-flight promise settle so the queue drains. */
const drain = () => act(async () => {});

beforeEach(() => {
  setRepoView.mockClear();
});

// Vitest runs without globals, so RTL cannot auto-register its cleanup — done
// here so one test's mounted hooks and listeners do not leak into the next.
afterEach(cleanup);

describe("useRepoView", () => {
  it("writes update locally immediately and send once to the server", async () => {
    const { result } = renderHook(() => useRepoView());

    act(() => result.current.remember("r1", aView));

    // Synchronously readable — a restore asks before the next render — and in
    // the rendered state too, which is what a later project switch reads.
    expect(result.current.rememberedFor("r1")).toEqual(aView);
    expect(result.current.viewOf("r1")).toEqual(aView);
    await drain();
    expect(setRepoView).toHaveBeenCalledTimes(1);
    expect(setRepoView).toHaveBeenCalledWith("r1", aView);
  });

  it("does not send the same view again", async () => {
    // Recording runs on every render, but most changes do not change the result.
    const { result } = renderHook(() => useRepoView());

    act(() => result.current.remember("r1", aView));
    await drain();
    act(() => result.current.remember("r1", { ...aView }));
    await drain();

    expect(setRepoView).toHaveBeenCalledTimes(1);
  });

  it("consecutive writes keep only the latest queued value and send in order", async () => {
    const { result } = renderHook(() => useRepoView());
    const third: RepoView = { ...blankView(), tab: "tree" };

    act(() => {
      result.current.remember("r1", aView);
      result.current.remember("r1", bView);
      result.current.remember("r1", third);
    });
    await drain();

    // The first request holds the slot; later queued values collapse to the latest one.
    expect(setRepoView.mock.calls.map(([, view]) => view)).toEqual([
      aView,
      third,
    ]);
  });

  it("covers distinguishes “none” from “not received yet”", () => {
    const { result } = renderHook(() => useRepoView());
    expect(result.current.covers("r1")).toBe(false);

    // The response included r1, so “none” is an answer even when no view is remembered.
    act(() => result.current.adopt({}, ["r1"]));
    expect(result.current.covers("r1")).toBe(true);
    expect(result.current.covers("r2")).toBe(false);
  });

  it("the covered set is replaced on every response", () => {
    // Repository ids live only for one process, so accumulating them could make a different project after restart
    // appear to have already been received under an old id.
    const { result } = renderHook(() => useRepoView());

    act(() => result.current.adopt({}, ["r1"]));
    act(() => result.current.adopt({}, ["r2"]));

    expect(result.current.covers("r1")).toBe(false);
    expect(result.current.covers("r2")).toBe(true);
  });

  it("adopt accepts the map sent by the server and does nothing when it is unchanged", () => {
    const { result } = renderHook(() => useRepoView());

    act(() => result.current.adopt({ r1: aView }, ["r1"]));
    expect(result.current.viewOf("r1")).toEqual(aView);

    // A new object with the same content has no changes, so state should stay the same.
    const held = result.current.viewOf("r1");
    act(() => result.current.adopt({ r1: { ...aView } }, ["r1"]));
    expect(result.current.viewOf("r1")).toBe(held);
  });

  it("local writes increment writes so the poll guard can distinguish them", () => {
    const { result } = renderHook(() => useRepoView());
    const before = result.current.writes.current;

    act(() => result.current.remember("r1", aView));

    expect(result.current.writes.current).toBe(before + 1);
  });
});

/**
 * The two hooks wired the way `useAppViewModel` wires them: `known` from
 * `covers`, `remembered` from `viewOf`, `latest` from `rememberedFor`. The
 * harness tests above each mock the other half, so only this catches the two
 * halves disagreeing — an `adopt` that updated the rendered state but not the
 * ref `latest` reads, say, would pass both and still erase fields here.
 */
describe("useRepoView + useRepoViewMemory integration wiring", () => {
  const setTab = vi.fn();
  const openDiff = vi.fn();

  beforeEach(() => {
    setTab.mockClear();
    openDiff.mockClear();
  });

  const wired = () =>
    renderHook(
      (props: { repo: string | null }) => {
        const view = useRepoView();
        const memory = useRepoViewMemory({
          repo: props.repo,
          known: view.covers(props.repo),
          remembered: view.viewOf(props.repo),
          latest: view.rememberedFor,
          remember: view.remember,
          setTab,
          openDiff,
          openFile: vi.fn(),
          openCommitFileDiff: vi.fn(),
        });
        return { view, memory };
      },
      { initialProps: { repo: "r1" as string | null } },
    );

  it("restores the view reported by poll and merges the current selection on top", async () => {
    const remembered: RepoView = {
      tab: "tree",
      file: workdirFile("a.ts", "diff"),
      tree_expanded: ["src"],
    };
    const { result } = wired();
    expect(openDiff).not.toHaveBeenCalled();

    // The poll lands: covers flips, the view arrives, the restore fires.
    act(() => result.current.view.adopt({ r1: remembered }, ["r1"]));
    expect(setTab).toHaveBeenCalledWith("tree");
    expect(openDiff).toHaveBeenCalledWith("a.ts", { restoring: true });

    // A choice merges into what adopt put there — through the real store, so
    // the untouched fields survive only if both halves of it agree.
    act(() => result.current.memory.noteFile(workdirFile("b.ts", "source")));
    await drain();
    expect(result.current.view.rememberedFor("r1")).toEqual({
      ...remembered,
      file: workdirFile("b.ts", "source"),
    });
    expect(setRepoView).toHaveBeenLastCalledWith("r1", {
      ...remembered,
      file: workdirFile("b.ts", "source"),
    });
  });
});
