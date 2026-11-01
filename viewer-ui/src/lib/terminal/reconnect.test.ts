import { describe, expect, it } from "vitest";
import {
  RECONNECT_BASE_MS,
  RECONNECT_MAX_MS,
  reconnectDelay,
} from "./reconnect";

const middle = () => 0.5;

describe("reconnectDelay", () => {
  it("doubles from half a second up to the ceiling", () => {
    expect([0, 1, 2, 3, 4, 5, 6].map((attempt) => reconnectDelay(attempt, middle)))
      .toEqual([500, 1_000, 2_000, 4_000, 8_000, 15_000, 15_000]);
  });

  it("spreads each wait around the doubling", () => {
    expect(reconnectDelay(2, () => 0)).toBe(1_600);
    expect(reconnectDelay(2, () => 1)).toBe(2_400);
  });

  it("never exceeds the ceiling even when the jitter is added to it", () => {
    expect(reconnectDelay(5, () => 1)).toBe(RECONNECT_MAX_MS);
    expect(reconnectDelay(50, () => 1)).toBe(RECONNECT_MAX_MS);
  });

  it("treats a nonsensical attempt as the first retry", () => {
    expect(reconnectDelay(-3, middle)).toBe(RECONNECT_BASE_MS);
    expect(reconnectDelay(Number.NaN, middle)).toBe(RECONNECT_BASE_MS);
    expect(reconnectDelay(1.7, middle)).toBe(1_000);
  });
});
