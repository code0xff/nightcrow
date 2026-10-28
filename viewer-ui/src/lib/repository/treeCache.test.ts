import { describe, expect, it } from "vitest";
import type { TreeEntry } from "../../api";
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
  it("stores lists by path", () => {
    const one = withChildren(emptyTreeCache, "src", entries("a.rs"));
    const two = withChildren(one, "lib", entries("b.ts"));

    expect(two.children).toEqual({
      src: entries("a.rs"),
      lib: entries("b.ts"),
    });
  });

  it("a newly read list replaces the previous list", () => {
    const first = withChildren(emptyTreeCache, "src", entries("a.rs"));

    const second = withChildren(first, "src", entries("a.rs", "b.rs"));

    expect(second.children.src).toEqual(entries("a.rs", "b.rs"));
  });

  it("writes do not mutate the previous cache", () => {
    const before = withChildren(emptyTreeCache, "src", entries("a.rs"));

    withToggled(withChildren(before, "lib", entries("b.ts")), "lib");

    expect(before.children).toEqual({ src: entries("a.rs") });
    expect(before.expanded.size).toBe(0);
    expect(emptyTreeCache.children).toEqual({});
  });

  it("toggle opens and closes", () => {
    const opened = withToggled(emptyTreeCache, "src");
    expect(opened.expanded.has("src")).toBe(true);
    expect(withToggled(opened, "src").expanded.has("src")).toBe(false);
  });

  it("reveal opens every ancestor of a path", () => {
    const revealed = withRevealed(emptyTreeCache, ancestorDirs("a/b/c"));

    expect([...revealed.expanded]).toEqual(["a", "a/b", "a/b/c"]);
  });

  it("reveal does not close an already open directory", () => {
    const opened = withToggled(emptyTreeCache, "a");

    const revealed = withRevealed(opened, ancestorDirs("a/b"));

    expect([...revealed.expanded]).toEqual(["a", "a/b"]);
  });

  it("the only ancestor of a top-level path is itself", () => {
    expect(ancestorDirs("src")).toEqual(["src"]);
  });
});
