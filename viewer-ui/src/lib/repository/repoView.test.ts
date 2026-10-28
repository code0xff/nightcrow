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
import type { RepoView } from "../../api";

describe("what a tap records", () => {
  it("a working-tree file is represented by its path and side", () => {
    expect(workdirFile("src/main.rs", "diff")).toEqual({
      path: "src/main.rs",
      commit: null,
      face: "diff",
    });
    expect(workdirFile("src/main.rs", "source").face).toBe("source");
  });

  it("a file opened from a commit also records that commit", () => {
    expect(commitFile("9a3bc2c", "src/app.rs", "diff")).toEqual({
      path: "src/app.rs",
      commit: "9a3bc2c",
      face: "diff",
    });
  });

  it("a project with no remembered view opens to an empty status view", () => {
    expect(blankView()).toEqual({ tab: "status", file: null, tree_expanded: [] });
  });
});

describe("cappedTree", () => {
  it("sorts values so equivalent views produce the same write", () => {
    expect(cappedTree(["src/ui", "src"])).toEqual(["src", "src/ui"]);
    expect(
      sameView(
        { ...blankView(), tree_expanded: cappedTree(["src/ui", "src"]) },
        { ...blankView(), tree_expanded: cappedTree(["src", "src/ui"]) },
      ),
    ).toBe(true);
  });

  it("truncates at the same boundary as the server", () => {
    // Without truncating, the server returns a shortened value, the client sends the full value again on every poll,
    // and the cycle repeats, leaving the two sides with different values forever.
    const many = Array.from(
      { length: MAX_TREE_EXPANDED + 20 },
      (_, i) => `dir${i}`,
    );
    expect(cappedTree(many)).toHaveLength(MAX_TREE_EXPANDED);
  });
});

describe("restoreOpen", () => {
  it("opens the remembered side", () => {
    expect(
      restoreOpen({ ...blankView(), file: workdirFile("a.ts", "source") }),
    ).toEqual({ kind: "file", path: "a.ts" });
    expect(
      restoreOpen({ ...blankView(), file: workdirFile("a.ts", "diff") }),
    ).toEqual({ kind: "diff", path: "a.ts" });
  });

  it("a committed file returns to diff regardless of its remembered side", () => {
    // The source side is one toggle away in the pane, and there is no direct opener in the panel.
    expect(
      restoreOpen({
        ...blankView(),
        tab: "log",
        file: commitFile("9a3bc2c", "a.ts", "source"),
      }),
    ).toEqual({ kind: "commitDiff", oid: "9a3bc2c", path: "a.ts" });
  });

  it("opens nothing when no view is remembered", () => {
    expect(restoreOpen(undefined)).toEqual({ kind: "none" });
    expect(restoreOpen(blankView())).toEqual({ kind: "none" });
    expect(restoreOpen({ ...blankView(), file: workdirFile("", "diff") })).toEqual({
      kind: "none",
    });
  });
});

describe("restoreTab", () => {
  it("opens the remembered tab", () => {
    for (const tab of ["status", "log", "tree"] as const) {
      expect(restoreTab({ ...blankView(), tab })).toBe(tab);
    }
  });

  it("opens status for an unknown tab or an empty view", () => {
    // The name may come from a newer server. Opening an unrenderable tab would
    // leave the sidebar empty.
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

  it("does not write the same view again", () => {
    expect(sameView(view, { ...view })).toBe(true);
  });

  it("writes when any value differs", () => {
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
