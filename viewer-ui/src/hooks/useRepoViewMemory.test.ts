// @vitest-environment happy-dom
//
// The regression net for the view-memory review rounds: every defect they shed
// lived in this hook's ordering — restore vs record, project switches, the
// window before the server's response. Each scenario here is one of them.

import { act, cleanup, renderHook } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { useRepoViewMemory } from "./useRepoViewMemory";
import { blankView, workdirFile } from "../lib/repoView";
import type { RepoView, RepoViewByRepo } from "../api";

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

describe("복원", () => {
  it("프로젝트를_열면_기억한_탭과_파일이_한_번_복원된다", () => {
    const h = harness({ repo: "r1", known: true, remembered: memoryOf("a.ts") });

    expect(h.setTab).toHaveBeenCalledWith("tree");
    expect(h.openDiff).toHaveBeenCalledWith("a.ts", { restoring: true });

    // 재렌더는 다시 열 이유가 아니다 — 방금 닫은 파일이 되돌아온다.
    h.rerender({ repo: "r1", known: true, remembered: memoryOf("a.ts") });
    expect(h.openDiff).toHaveBeenCalledTimes(1);
  });

  it("떠났다_돌아오면_다시_복원된다", () => {
    // 전환이 pane을 비웠으므로, 복귀는 "프로젝트를 여는 것"과 같다.
    const h = harness({ repo: "r1", known: true, remembered: memoryOf("a.ts") });
    h.rerender({ repo: "r2", known: true, remembered: undefined });
    h.rerender({ repo: "r1", known: true, remembered: memoryOf("a.ts") });

    expect(h.openDiff).toHaveBeenCalledTimes(2);
  });

  it("응답이_오기_전에는_복원하지_않고_오면_한다", () => {
    // "기억 없음"과 "아직 못 들음"은 맵에서 구분되지 않는다 — known이 가른다.
    const h = harness({ repo: "r1", known: false, remembered: undefined });
    expect(h.setTab).not.toHaveBeenCalled();

    h.rerender({ repo: "r1", known: true, remembered: memoryOf("a.ts") });
    expect(h.openDiff).toHaveBeenCalledWith("a.ts", { restoring: true });
  });

  it("기억이_없어도_탭은_status로_되돌린다", () => {
    // 아니면 자기 뷰가 없는 프로젝트가 직전 프로젝트의 탭을 물려받는다.
    const h = harness({ repo: "r1", known: true, remembered: undefined });
    expect(h.setTab).toHaveBeenCalledWith("status");
    expect(h.openDiff).not.toHaveBeenCalled();
  });

  it("아직_못_들은_프로젝트를_거쳐도_다음_프로젝트는_복원된다", () => {
    // A → (막 연) B → A: B에서 복원 못 했다는 사실이 A의 복원을 막으면 안 된다.
    const h = harness({ repo: "r1", known: true, remembered: memoryOf("a.ts") });
    h.rerender({ repo: "r2", known: false, remembered: undefined });
    h.rerender({ repo: "r1", known: true, remembered: memoryOf("a.ts") });

    expect(h.openDiff).toHaveBeenCalledTimes(2);
  });

  it("기억한_면대로_맞는_opener가_불린다", () => {
    // 순수 함수 restoreOpen의 분기가 아니라, 훅이 그 답을 어느 opener에
    // 배선했는지를 고정한다.
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

  it("A에서_고르고_B로_가면_B는_제_기억대로_복원된다", () => {
    // touched는 방문의 속성이다: A에서의 선택이 B의 복원을 막으면 안 된다.
    const h = harness({ repo: "r1", known: true, remembered: undefined });
    act(() => h.result.current.noteFile(workdirFile("mine.ts", "diff")));

    h.rerender({ repo: "r2", known: true, remembered: memoryOf("b.ts") });

    expect(h.openDiff).toHaveBeenCalledWith("b.ts", { restoring: true });
    expect(h.result.current.touched).toBe(false);
  });

  it("복원은_아무것도_기록하지_않는다", () => {
    // 이미 저장된 것을 되돌리는 일이다. 기록하면 실패가 기억을 지울 길이 생긴다.
    const h = harness({ repo: "r1", known: true, remembered: memoryOf("a.ts") });
    expect(h.remember).not.toHaveBeenCalled();
  });
});

describe("기록", () => {
  it("선택은_지금_들고_있는_값에_합쳐져_한_틱의_두_선택이_안_지운다", () => {
    // 탭 전환은 탭 *그리고* 그 탭이 비우는 pane — 렌더 사본에 각각 합치면
    // 뒤가 앞을 지운다.
    const h = harness({ repo: "r1", known: true, remembered: undefined });

    act(() => {
      h.result.current.noteTab("log");
      h.result.current.noteFile(null);
    });

    expect(h.store.r1).toEqual({ ...blankView(), tab: "log", file: null });
  });

  it("파일_선택은_다른_항목의_기억을_지우지_않는다", () => {
    const h = harness(
      { repo: "r1", known: true, remembered: memoryOf("a.ts") },
      { r1: memoryOf("a.ts") },
    );

    act(() => h.result.current.noteFile(workdirFile("b.ts", "source")));

    // 트리 모양과 탭은 그대로, 파일만 바뀐다.
    expect(h.store.r1.tab).toBe("tree");
    expect(h.store.r1.file).toEqual(workdirFile("b.ts", "source"));
  });
});

describe("응답 전의 선택", () => {
  it("보관됐다가_응답_위에_얹히고_복원은_그_위에_하지_않는다", () => {
    const h = harness({ repo: "r1", known: false, remembered: undefined });

    act(() => h.result.current.noteFile(workdirFile("mine.ts", "diff")));
    expect(h.remember).not.toHaveBeenCalled();

    h.rerender({ repo: "r1", known: true, remembered: memoryOf("old.ts") });

    // 고른 파일이 기억 위에 얹히고, 안 고른 트리 모양은 옛 답을 유지한다.
    expect(h.store.r1.file).toEqual(workdirFile("mine.ts", "diff"));
    expect(h.store.r1.tab).toBe("tree");
    // 이미 쓰고 있는 화면이니 복원으로 덮지 않는다.
    expect(h.openDiff).not.toHaveBeenCalled();
    expect(h.result.current.touched).toBe(true);
  });

  it("답을_기다리다_떠나도_선택은_프로젝트에_남아_복귀_때_반영된다", () => {
    // 자리를 뜨는 건 마음이 바뀐 게 아니다.
    const h = harness({ repo: "r1", known: false, remembered: undefined });
    act(() => h.result.current.noteTab("log"));

    h.rerender({ repo: "r2", known: true, remembered: undefined });
    h.rerender({ repo: "r1", known: true, remembered: undefined });

    expect(h.store.r1.tab).toBe("log");
  });

  it("한_프로젝트의_보류가_다른_프로젝트에_새지_않는다", () => {
    const h = harness({ repo: "r1", known: false, remembered: undefined });
    act(() => h.result.current.noteTab("log"));

    // r2가 먼저 응답을 받는다 — r1의 선택은 r2와 무관해야 한다.
    h.rerender({ repo: "r2", known: true, remembered: undefined });
    expect(h.store.r2).toBeUndefined();
  });
});
