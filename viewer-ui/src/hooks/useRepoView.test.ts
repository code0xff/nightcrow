// @vitest-environment happy-dom

import { act, cleanup, renderHook } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { useRepoView } from "./useRepoView";
import { useRepoViewMemory } from "./useRepoViewMemory";
import { blankView, workdirFile } from "../lib/repoView";
import type { RepoView } from "../api";

vi.mock("../api", () => ({
  api: { setRepoView: vi.fn(() => Promise.resolve({})) },
}));

// The mock above replaces the module, so this import resolves to it.
import { api } from "../api";
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
  it("기록은_로컬에_즉시_반영되고_서버로_한_번_나간다", async () => {
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

  it("같은_뷰를_다시_기록하면_보내지_않는다", async () => {
    // 기록은 화면이 바뀔 때마다 불리고, 대부분의 변화는 답을 안 바꾼다.
    const { result } = renderHook(() => useRepoView());

    act(() => result.current.remember("r1", aView));
    await drain();
    act(() => result.current.remember("r1", { ...aView }));
    await drain();

    expect(setRepoView).toHaveBeenCalledTimes(1);
  });

  it("연속_기록은_최신만_남기고_순서대로_나간다", async () => {
    const { result } = renderHook(() => useRepoView());
    const third: RepoView = { ...blankView(), tab: "tree" };

    act(() => {
      result.current.remember("r1", aView);
      result.current.remember("r1", bView);
      result.current.remember("r1", third);
    });
    await drain();

    // 첫 요청이 슬롯을 잡고, 그 뒤에 쌓인 것은 최신 하나로 접힌다.
    expect(setRepoView.mock.calls.map(([, view]) => view)).toEqual([
      aView,
      third,
    ]);
  });

  it("covers는_없다와_아직_못_들었다를_가른다", () => {
    const { result } = renderHook(() => useRepoView());
    expect(result.current.covers("r1")).toBe(false);

    // 응답이 r1을 담아 말했다 — 기억이 없어도 "없다"는 답이 된 것이다.
    act(() => result.current.adopt({}, ["r1"]));
    expect(result.current.covers("r1")).toBe(true);
    expect(result.current.covers("r2")).toBe(false);
  });

  it("covered_집합은_응답마다_교체된다", () => {
    // repo id는 프로세스 수명뿐이라, 누적하면 재시작 후 다른 프로젝트가
    // 옛 id로 "이미 들었다"가 된다.
    const { result } = renderHook(() => useRepoView());

    act(() => result.current.adopt({}, ["r1"]));
    act(() => result.current.adopt({}, ["r2"]));

    expect(result.current.covers("r1")).toBe(false);
    expect(result.current.covers("r2")).toBe(true);
  });

  it("adopt는_서버가_보낸_맵을_받아들이되_같으면_아무_일도_없다", () => {
    const { result } = renderHook(() => useRepoView());

    act(() => result.current.adopt({ r1: aView }, ["r1"]));
    expect(result.current.viewOf("r1")).toEqual(aView);

    // 같은 내용의 새 객체 — 바뀐 게 없으니 상태도 그대로여야 한다.
    const held = result.current.viewOf("r1");
    act(() => result.current.adopt({ r1: { ...aView } }, ["r1"]));
    expect(result.current.viewOf("r1")).toBe(held);
  });

  it("로컬_기록은_writes를_올려_poll_guard가_구분할_수_있다", () => {
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
describe("useRepoView + useRepoViewMemory, 실배선", () => {
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

  it("poll이_말해준_기억이_복원되고_선택은_그_위에_합쳐진다", async () => {
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
