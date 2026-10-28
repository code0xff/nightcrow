import { describe, expect, it } from "vitest";
import {
  DEFAULT_UPPER_PCT,
  MAX_UPPER_PCT,
  MIN_UPPER_PCT,
  clampUpperPct,
  clampUpperPctExact,
  upperPctAt,
} from "./upperPct";

describe("clampUpperPct", () => {
  it("returns an in-range value rounded to an integer", () => {
    expect(clampUpperPct(55)).toBe(55);
    expect(clampUpperPct(54.6)).toBe(55);
  });

  it("clamps an out-of-range value to the nearest boundary", () => {
    expect(clampUpperPct(0)).toBe(MIN_UPPER_PCT);
    expect(clampUpperPct(-40)).toBe(MIN_UPPER_PCT);
    expect(clampUpperPct(100)).toBe(MAX_UPPER_PCT);
  });
});

describe("clampUpperPctExact", () => {
  it("clamps the value without rounding", () => {
    expect(clampUpperPctExact(54.6)).toBe(54.6);
    expect(clampUpperPctExact(100)).toBe(MAX_UPPER_PCT);
  });

  it("replaces non-finite values with the default", () => {
    // A server response or localStorage value can be NaN; if it passes through,
    // the grid track becomes `NaNfr` and breaks the layout.
    for (const bad of [NaN, Infinity, -Infinity]) {
      expect(clampUpperPctExact(bad)).toBe(DEFAULT_UPPER_PCT);
    }
    expect(clampUpperPct(NaN)).toBe(DEFAULT_UPPER_PCT);
  });
});

describe("upperPctAt", () => {
  it("maps the pointer position to a ratio within the range", () => {
    // The midpoint of the 200..1000 range.
    expect(upperPctAt(600, 200, 1000, DEFAULT_UPPER_PCT)).toBe(50);
  });

  it("does not round, so it follows the pointer precisely", () => {
    // In a 1000px range, moving 1px changes 0.1%. Rounding makes the divider lose the pointer
    // and move in one-percent steps.
    expect(upperPctAt(501, 0, 1000, DEFAULT_UPPER_PCT)).toBeCloseTo(50.1);
  });

  it("dragging outside the range does not cross its boundary", () => {
    expect(upperPctAt(-500, 0, 800, DEFAULT_UPPER_PCT)).toBe(MIN_UPPER_PCT);
    expect(upperPctAt(5000, 0, 800, DEFAULT_UPPER_PCT)).toBe(MAX_UPPER_PCT);
  });

  it("keeps the current value when the range height is zero", () => {
    // Avoid division by zero before the region has been laid out.
    expect(upperPctAt(400, 500, 500, 40)).toBe(40);
  });

  it("clamps the current value to the range when height is zero", () => {
    expect(upperPctAt(400, 500, 500, 0)).toBe(MIN_UPPER_PCT);
  });
});
