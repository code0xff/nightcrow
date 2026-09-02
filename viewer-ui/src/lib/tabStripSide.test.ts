import { describe, expect, it } from "vitest";
import { otherSide, parseTabStripSide } from "./tabStripSide";

describe("parseTabStripSide", () => {
  it("두_자리_이름을_그대로_읽는다", () => {
    expect(parseTabStripSide("top")).toBe("top");
    expect(parseTabStripSide("left")).toBe("left");
  });

  it("모르는_값은_null이다", () => {
    // A future version's name, a typo, or nothing stored: the caller falls
    // back to the default rather than trusting the string.
    expect(parseTabStripSide(null)).toBeNull();
    expect(parseTabStripSide("")).toBeNull();
    expect(parseTabStripSide("bottom")).toBeNull();
    expect(parseTabStripSide("Left")).toBeNull();
  });
});

describe("otherSide", () => {
  it("두_자리를_서로_바꾼다", () => {
    expect(otherSide("top")).toBe("left");
    expect(otherSide("left")).toBe("top");
  });
});
