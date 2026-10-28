import { describe, expect, it } from "vitest";
import { neighborRepo } from "./projectCycle";

describe("neighborRepo", () => {
  it("the next project is the next item in order", () => {
    expect(neighborRepo(["a", "b", "c"], "a", 1)).toBe("b");
    expect(neighborRepo(["a", "b", "c"], "b", 1)).toBe("c");
  });

  it("the previous project is the previous item in order", () => {
    expect(neighborRepo(["a", "b", "c"], "c", -1)).toBe("b");
    expect(neighborRepo(["a", "b", "c"], "b", -1)).toBe("a");
  });

  it("next wraps from the last project to the first", () => {
    expect(neighborRepo(["a", "b", "c"], "c", 1)).toBe("a");
  });

  it("previous wraps from the first project to the last", () => {
    expect(neighborRepo(["a", "b", "c"], "a", -1)).toBe("c");
  });

  it("with two projects, both directions select the other project", () => {
    expect(neighborRepo(["a", "b"], "a", 1)).toBe("b");
    expect(neighborRepo(["a", "b"], "a", -1)).toBe("b");
    expect(neighborRepo(["a", "b"], "b", 1)).toBe("a");
    expect(neighborRepo(["a", "b"], "b", -1)).toBe("a");
  });

  it("there is nowhere to move when only one project exists", () => {
    expect(neighborRepo(["a"], "a", 1)).toBeNull();
    expect(neighborRepo(["a"], "a", -1)).toBeNull();
  });

  it("there is nowhere to move when the list is empty", () => {
    expect(neighborRepo([], null, 1)).toBeNull();
    expect(neighborRepo([], "a", -1)).toBeNull();
  });

  it("does nothing when there is no selection", () => {
    expect(neighborRepo(["a", "b"], null, 1)).toBeNull();
  });

  it("does nothing when the selection is not in the list", () => {
    // The poll is still resolving; jumping to the first tab would race it.
    expect(neighborRepo(["a", "b"], "gone", 1)).toBeNull();
    expect(neighborRepo(["a", "b"], "gone", -1)).toBeNull();
  });

  it("uses the first occurrence when there are duplicates", () => {
    // The tab list is unique, but keep this helper's duplicate-id behavior fixed.
    expect(neighborRepo(["a", "b", "a"], "a", 1)).toBe("b");
  });
});
