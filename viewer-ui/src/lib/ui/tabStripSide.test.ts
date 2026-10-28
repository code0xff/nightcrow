import { describe, expect, it } from "vitest";
import { otherSide, parseTabStripSide } from "./tabStripSide";

describe("parseTabStripSide", () => {
  it("reads both side names unchanged", () => {
    expect(parseTabStripSide("top")).toBe("top");
    expect(parseTabStripSide("left")).toBe("left");
  });

  it("an unknown value is null", () => {
    // A future version's name, a typo, or nothing stored: the caller falls
    // back to the default rather than trusting the string.
    expect(parseTabStripSide(null)).toBeNull();
    expect(parseTabStripSide("")).toBeNull();
    expect(parseTabStripSide("bottom")).toBeNull();
    expect(parseTabStripSide("Left")).toBeNull();
  });
});

describe("otherSide", () => {
  it("swaps the two sides", () => {
    expect(otherSide("top")).toBe("left");
    expect(otherSide("left")).toBe("top");
  });
});
