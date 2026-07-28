import { describe, expect, it } from "vitest";
import type { TreeEntry } from "../api";
import {
  ancestorDirs,
  emptyTreeCache,
  forRepo,
  withChildren,
  withRevealed,
  withToggled,
} from "./treeCache";

const entries = (...names: string[]): TreeEntry[] =>
  names.map((name) => ({ name, is_dir: false }));

describe("tree cache", () => {
  it("다른_프로젝트로_바뀌면_같은_경로의_목록도_버린다", () => {
    // `src` exists in both projects. Keeping the listing would show one
    // project's files under the other, and the tree would not refetch a path
    // it already holds.
    const a = withChildren(emptyTreeCache, "r1", "src", entries("a.rs"));

    const b = forRepo(a, "r2");

    expect(b.children).toEqual({});
    expect(b.repo).toBe("r2");
  });

  it("펼쳐둔_디렉토리도_프로젝트를_따라간다", () => {
    const a = withToggled(withChildren(emptyTreeCache, "r1", "", []), "src");

    const b = forRepo(a, "r2");

    expect(b.expanded.has("src")).toBe(false);
  });

  it("같은_프로젝트면_목록을_유지한다", () => {
    const a = withChildren(emptyTreeCache, "r1", "src", entries("a.rs"));

    expect(forRepo(a, "r1")).toBe(a);
  });

  it("전환_전에_보낸_응답은_새_프로젝트_캐시에_섞이지_않는다", () => {
    // The listing for r1 was requested before the switch and arrives after it.
    const current = withChildren(emptyTreeCache, "r2", "src", entries("b.ts"));

    const late = withChildren(current, "r1", "src", entries("a.rs"));

    expect(late.repo).toBe("r1");
    expect(late.children).toEqual({ src: entries("a.rs") });
    expect(current.children).toEqual({ src: entries("b.ts") });
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
