import { describe, expect, it } from "vitest";
import type { TreeEntry } from "../api";
import {
  ancestorDirs,
  emptyMatches,
  emptyTreeCache,
  forOwner,
  matchesFor,
  visibleFor,
  withChildren,
  withRevealed,
  withToggled,
} from "./treeCache";

const entries = (...names: string[]): TreeEntry[] =>
  names.map((name) => ({ name, is_dir: false }));

/** A cache holding one listing for `owner`, as the hook would build it. */
const loaded = (
  owner: { repo: string | null },
  path: string,
  names: string[],
) => withChildren(emptyTreeCache, owner, owner, path, entries(...names));

describe("tree cache", () => {
  it("다른_프로젝트로_바뀌면_같은_경로의_목록도_버린다", () => {
    // `src` exists in both projects. Keeping the listing would show one
    // project's files under the other, and the tree would not refetch a path
    // it already holds.
    const a = loaded({ repo: "r1" }, "src", ["a.rs"]);

    const b = forOwner(a, { repo: "r2" });

    expect(b.children).toEqual({});
    expect(b.owner?.repo).toBe("r2");
  });

  it("펼쳐둔_디렉토리도_프로젝트를_따라간다", () => {
    const a = withToggled(loaded({ repo: "r1" }, "", []), "src");

    const b = forOwner(a, { repo: "r2" });

    expect(b.expanded.has("src")).toBe(false);
  });

  it("같은_방문이면_목록을_유지한다", () => {
    const visit = { repo: "r1" };
    const a = loaded(visit, "src", ["a.rs"]);

    expect(forOwner(a, visit)).toBe(a);
  });

  it("전환_뒤에_도착한_이전_프로젝트의_목록은_버려진다", () => {
    // Requested for r1 before the switch, delivered after it. Storing it would
    // put r1's files back on screen and undo the reset.
    const current = loaded({ repo: "r2" }, "src", ["b.ts"]);

    const late = withChildren(
      current,
      { repo: "r2" },
      { repo: "r1" },
      "src",
      entries("a.rs"),
    );

    expect(late).toBe(current);
  });

  it("같은_프로젝트를_다시_열면_이전_방문의_목록은_버려진다", () => {
    // r1 → r2 → r1: the first visit's request can land after the second visit
    // has loaded the same path, and by name alone it looks current. Visits are
    // told apart by identity so the newer listing survives.
    const first = { repo: "r1" };
    const second = { repo: "r1" };
    const current = loaded(second, "src", ["new.rs"]);

    const late = withChildren(current, second, first, "src", entries("old.rs"));

    expect(late).toBe(current);
  });

  it("프로젝트를_보고_있지_않으면_어떤_목록도_받지_않는다", () => {
    const none = withChildren(
      emptyTreeCache,
      null,
      { repo: "r1" },
      "src",
      entries("a.rs"),
    );

    expect(none).toBe(emptyTreeCache);
  });

  it("전환_직후_새_프로젝트의_첫_목록은_이전_캐시를_대체한다", () => {
    // The cache can still belong to r1's visit when r2's first listing lands;
    // it must start r2's cache rather than join r1's.
    const stale = loaded({ repo: "r1" }, "src", ["a.rs"]);
    const now = { repo: "r2" };

    const fresh = withChildren(stale, now, now, "lib", entries("b.ts"));

    expect(fresh.owner).toBe(now);
    expect(fresh.children).toEqual({ lib: entries("b.ts") });
  });

  it("화면은_다른_프로젝트의_목록을_그리지_않는다", () => {
    const a = loaded({ repo: "r1" }, "src", ["a.rs"]);

    expect(visibleFor(a, "r2").children).toEqual({});
    expect(visibleFor(a, "r1")).toBe(a);
  });

  it("같은_프로젝트의_이전_방문_캐시는_전환_전까지_계속_보인다", () => {
    // Rendering compares by name: between the render that switches project and
    // the effect that starts the new visit the cache still belongs to the old
    // visit, and hiding it there would blank the tree for a frame.
    const previous = loaded({ repo: "r1" }, "src", ["a.rs"]);

    expect(visibleFor(previous, "r1")).toBe(previous);
  });

  it("검색_결과도_자기_프로젝트에서만_보인다", () => {
    // Search results are repository-relative paths too, and clearing them in
    // an effect would leave one frame of the previous project's matches.
    const found = { owner: { repo: "r1" }, items: ["src/a.rs"], truncated: true };

    expect(matchesFor(found, "r1")).toEqual({
      items: ["src/a.rs"],
      truncated: true,
    });
    expect(matchesFor(found, "r2")).toEqual({ items: [], truncated: false });
    expect(matchesFor(emptyMatches<string>(), null)).toEqual({
      items: [],
      truncated: false,
    });
  });

  it("토글은_열고_닫는다", () => {
    const opened = withToggled(emptyTreeCache, "src");
    expect(opened.expanded.has("src")).toBe(true);
    expect(withToggled(opened, "src").expanded.has("src")).toBe(false);
  });

  it("reveal은_경로의_모든_상위를_연다", () => {
    const revealed = withRevealed(emptyTreeCache, ancestorDirs("a/b/c"));

    expect([...revealed.expanded]).toEqual(["a", "a/b", "a/b/c"]);
  });

  it("최상위_경로의_조상은_자기_자신뿐이다", () => {
    expect(ancestorDirs("src")).toEqual(["src"]);
  });
});
