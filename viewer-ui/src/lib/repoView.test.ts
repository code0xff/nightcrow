import { describe, expect, it } from "vitest";
import {
  MAX_TREE_EXPANDED,
  blankView,
  cappedTree,
  commitFile,
  restoreOpen,
  restoreTab,
  sameView,
  workdirFile,
} from "./repoView";
import type { RepoView } from "../api";

describe("what a tap records", () => {
  it("워킹트리_파일은_경로와_어느_면인지로_남는다", () => {
    expect(workdirFile("src/main.rs", "diff")).toEqual({
      path: "src/main.rs",
      commit: null,
      face: "diff",
    });
    expect(workdirFile("src/main.rs", "source").face).toBe("source");
  });

  it("커밋에서_연_파일은_그_커밋도_함께_남는다", () => {
    expect(commitFile("9a3bc2c", "src/app.rs", "diff")).toEqual({
      path: "src/app.rs",
      commit: "9a3bc2c",
      face: "diff",
    });
  });

  it("아무것도_안_본_프로젝트는_status_빈_화면이다", () => {
    expect(blankView()).toEqual({ tab: "status", file: null, tree_expanded: [] });
  });
});

describe("cappedTree", () => {
  it("정렬해_같은_모양이_같은_쓰기가_되게_한다", () => {
    expect(cappedTree(["src/ui", "src"])).toEqual(["src", "src/ui"]);
    expect(
      sameView(
        { ...blankView(), tree_expanded: cappedTree(["src/ui", "src"]) },
        { ...blankView(), tree_expanded: cappedTree(["src", "src/ui"]) },
      ),
    ).toBe(true);
  });

  it("서버가_자르는_곳에서_같이_자른다", () => {
    // 안 자르면 서버가 잘라 돌려주고, 클라이언트는 다시 다 보내고, poll마다
    // 반복된다 — 두 쪽이 영원히 다른 값을 들고 있게 된다.
    const many = Array.from(
      { length: MAX_TREE_EXPANDED + 20 },
      (_, i) => `dir${i}`,
    );
    expect(cappedTree(many)).toHaveLength(MAX_TREE_EXPANDED);
  });
});

describe("restoreOpen", () => {
  it("기억한_면대로_연다", () => {
    expect(
      restoreOpen({ ...blankView(), file: workdirFile("a.ts", "source") }),
    ).toEqual({ kind: "file", path: "a.ts" });
    expect(
      restoreOpen({ ...blankView(), file: workdirFile("a.ts", "diff") }),
    ).toEqual({ kind: "diff", path: "a.ts" });
  });

  it("커밋의_파일은_어느_면이었든_diff로_돌아온다", () => {
    // 소스 면은 pane의 토글 한 번 거리이고, 곧장 여는 opener는 패널에 없다.
    expect(
      restoreOpen({
        ...blankView(),
        tab: "log",
        file: commitFile("9a3bc2c", "a.ts", "source"),
      }),
    ).toEqual({ kind: "commitDiff", oid: "9a3bc2c", path: "a.ts" });
  });

  it("기억한_것이_없으면_아무것도_열지_않는다", () => {
    expect(restoreOpen(undefined)).toEqual({ kind: "none" });
    expect(restoreOpen(blankView())).toEqual({ kind: "none" });
    expect(restoreOpen({ ...blankView(), file: workdirFile("", "diff") })).toEqual({
      kind: "none",
    });
  });
});

describe("restoreTab", () => {
  it("기억한_탭으로_연다", () => {
    for (const tab of ["status", "log", "tree"] as const) {
      expect(restoreTab({ ...blankView(), tab })).toBe(tab);
    }
  });

  it("모르는_탭과_빈_기억은_status로_연다", () => {
    // 더 새로운 서버가 보낸 이름일 수 있다. 아무것도 못 그리는 탭으로 열면
    // 사이드바가 빈 채로 남는다.
    expect(restoreTab(undefined)).toBe("status");
    expect(restoreTab({ ...blankView(), tab: "graph" as RepoView["tab"] })).toBe(
      "status",
    );
  });
});

describe("sameView", () => {
  const view: RepoView = {
    tab: "tree",
    file: workdirFile("a.ts", "source"),
    tree_expanded: ["src"],
  };

  it("같은_화면은_다시_쓰지_않는다", () => {
    expect(sameView(view, { ...view })).toBe(true);
  });

  it("무엇_하나라도_다르면_쓴다", () => {
    expect(sameView(undefined, view)).toBe(false);
    expect(sameView(view, { ...view, tab: "status" })).toBe(false);
    expect(sameView(view, { ...view, tree_expanded: ["src", "src/ui"] })).toBe(false);
    expect(sameView(view, { ...view, file: null })).toBe(false);
    expect(sameView(view, { ...view, file: workdirFile("a.ts", "diff") })).toBe(false);
    expect(
      sameView(view, { ...view, file: commitFile("9a3", "a.ts", "source") }),
    ).toBe(false);
  });
});
