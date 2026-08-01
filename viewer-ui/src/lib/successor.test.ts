import { describe, expect, it } from "vitest";
import { successorOf } from "./successor";

describe("successorOf", () => {
  it("takes the tab after the one closing", () => {
    expect(successorOf(["a", "b", "c"], "b")).toBe("c");
  });

  it("falls back to the tab before when the last one closes", () => {
    expect(successorOf(["a", "b", "c"], "c")).toBe("b");
  });

  it("has nothing to move to when the only tab closes", () => {
    expect(successorOf(["a"], "a")).toBeNull();
    expect(successorOf([], "a")).toBeNull();
  });

  it("takes the first tab when the closing one is not in the order", () => {
    // A close racing another client's: whatever is left is a better answer than
    // nothing, and the server's next word settles it either way.
    expect(successorOf(["a", "b"], "z")).toBe("a");
  });
});
