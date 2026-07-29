import { describe, expect, it } from "vitest";
import type { TreeEntry } from "../api";
import {
  ancestorDirs,
  emptyTreeCache,
  withChildren,
  withRevealed,
  withToggled,
} from "./treeCache";

const entries = (...names: string[]): TreeEntry[] =>
  names.map((name) => ({ name, is_dir: false }));

describe("tree cache", () => {
  it("목록은_경로별로_쌓인다", () => {
    const one = withChildren(emptyTreeCache, "src", entries("a.rs"));
    const two = withChildren(one, "lib", entries("b.ts"));

    expect(two.children).toEqual({
      src: entries("a.rs"),
      lib: entries("b.ts"),
    });
  });

  it("다시_읽은_목록이_이전_목록을_대체한다", () => {
    const first = withChildren(emptyTreeCache, "src", entries("a.rs"));

    const second = withChildren(first, "src", entries("a.rs", "b.rs"));

    expect(second.children.src).toEqual(entries("a.rs", "b.rs"));
  });

  it("쓰기는_이전_캐시를_바꾸지_않는다", () => {
    const before = withChildren(emptyTreeCache, "src", entries("a.rs"));

    withToggled(withChildren(before, "lib", entries("b.ts")), "lib");

    expect(before.children).toEqual({ src: entries("a.rs") });
    expect(before.expanded.size).toBe(0);
    expect(emptyTreeCache.children).toEqual({});
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

  it("reveal은_이미_열린_디렉토리를_닫지_않는다", () => {
    const opened = withToggled(emptyTreeCache, "a");

    const revealed = withRevealed(opened, ancestorDirs("a/b"));

    expect([...revealed.expanded]).toEqual(["a", "a/b"]);
  });

  it("최상위_경로의_조상은_자기_자신뿐이다", () => {
    expect(ancestorDirs("src")).toEqual(["src"]);
  });
});
