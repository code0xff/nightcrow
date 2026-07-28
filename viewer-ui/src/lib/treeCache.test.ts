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

/** A cache holding one listing for `repo`, as the hook would build it. */
const loaded = (repo: string, path: string, names: string[]) =>
  withChildren(emptyTreeCache, repo, repo, path, entries(...names));

describe("tree cache", () => {
  it("다른_프로젝트로_바뀌면_같은_경로의_목록도_버린다", () => {
    // `src` exists in both projects. Keeping the listing would show one
    // project's files under the other, and the tree would not refetch a path
    // it already holds.
    const a = loaded("r1", "src", ["a.rs"]);

    const b = forRepo(a, "r2");

    expect(b.children).toEqual({});
    expect(b.repo).toBe("r2");
  });

  it("펼쳐둔_디렉토리도_프로젝트를_따라간다", () => {
    const a = withToggled(loaded("r1", "", []), "src");

    const b = forRepo(a, "r2");

    expect(b.expanded.has("src")).toBe(false);
  });

  it("같은_프로젝트면_목록을_유지한다", () => {
    const a = loaded("r1", "src", ["a.rs"]);

    expect(forRepo(a, "r1")).toBe(a);
  });

  it("전환_뒤에_도착한_이전_프로젝트의_목록은_버려진다", () => {
    // Requested for r1 before the switch, delivered after it. Storing it would
    // put r1's files back on screen and undo the reset.
    const current = loaded("r2", "src", ["b.ts"]);

    const late = withChildren(current, "r2", "r1", "src", entries("a.rs"));

    expect(late).toBe(current);
  });

  it("프로젝트를_보고_있지_않으면_어떤_목록도_받지_않는다", () => {
    const none = withChildren(emptyTreeCache, null, "r1", "src", entries("a.rs"));

    expect(none).toBe(emptyTreeCache);
  });

  it("전환_직후_새_프로젝트의_첫_목록은_이전_캐시를_대체한다", () => {
    // The cache can still be tagged r1 when r2's first listing lands; it must
    // start r2's cache rather than join r1's.
    const stale = loaded("r1", "src", ["a.rs"]);

    const fresh = withChildren(stale, "r2", "r2", "lib", entries("b.ts"));

    expect(fresh.repo).toBe("r2");
    expect(fresh.children).toEqual({ lib: entries("b.ts") });
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
