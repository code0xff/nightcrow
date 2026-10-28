import { describe, expect, it } from "vitest";
import {
  anyHot,
  classifyHot,
  CLOCK_SKEW_EPSILON_MS,
  nextClockOffset,
} from "./hot";

const WINDOW = 10_000;
const NOW = 100_000;

describe("classifyHot", () => {
  it("classifies by age as fresh, warm, or cool", () => {
    expect(classifyHot(NOW - 1_000, NOW, WINDOW)).toBe("fresh");
    expect(classifyHot(NOW - 6_000, NOW, WINDOW)).toBe("warm");
    expect(classifyHot(NOW - 20_000, NOW, WINDOW)).toBe("cool");
  });

  it("the window boundary is cool", () => {
    expect(classifyHot(NOW - WINDOW, NOW, WINDOW)).toBe("cool");
    expect(classifyHot(NOW - WINDOW + 1, NOW, WINDOW)).toBe("warm");
  });

  it("treats a future mtime as clock skew and classifies it as fresh", () => {
    expect(classifyHot(NOW + 30_000, NOW, WINDOW)).toBe("fresh");
  });

  it("is cool when mtime is missing", () => {
    expect(classifyHot(undefined, NOW, WINDOW)).toBe("cool");
  });
});

describe("anyHot", () => {
  it("returns true if any entry is within the window", () => {
    expect(anyHot([undefined, NOW - 50_000, NOW - 2_000], NOW, WINDOW)).toBe(
      true,
    );
  });

  it("returns false when all entries are cool or the list is empty", () => {
    expect(anyHot([undefined, NOW - 50_000], NOW, WINDOW)).toBe(false);
    expect(anyHot([], NOW, WINDOW)).toBe(false);
  });
});

describe("nextClockOffset", () => {
  it("accepts the first measurement regardless of size", () => {
    expect(nextClockOffset(null, NOW + 30_000, NOW)).toBe(30_000);
    expect(nextClockOffset(null, NOW - 30_000, NOW)).toBe(-30_000);
    // Accepts the first value even when it is below epsilon. Epsilon suppresses noise between measurements,
    // not a threshold for deciding whether to calibrate.
    expect(nextClockOffset(null, NOW + 900, NOW)).toBe(900);
  });

  it("does not calibrate when server time is missing or invalid", () => {
    expect(nextClockOffset(null, undefined, NOW)).toBe(0);
    expect(nextClockOffset(null, 0, NOW)).toBe(0);
  });

  it("keeps the current value when it moves by less than one tick", () => {
    const held = 30_000;
    const jitter = CLOCK_SKEW_EPSILON_MS - 1;
    expect(nextClockOffset(held, NOW + held + jitter, NOW)).toBe(held);
  });

  it("uses the new value when it moves by at least one tick", () => {
    const held = 30_000;
    expect(nextClockOffset(held, NOW + held + CLOCK_SKEW_EPSILON_MS, NOW)).toBe(
      held + CLOCK_SKEW_EPSILON_MS,
    );
  });

  it("calibration gives the same classification when the clock is slow", () => {
    // The device clock is 30 seconds slow; without calibration, even files outside the window appear fresh.
    const behind = NOW - 30_000;
    expect(classifyHot(NOW - 20_000, behind, WINDOW)).toBe("fresh");
    const offset = nextClockOffset(null, NOW, behind);
    expect(classifyHot(NOW - 20_000, behind + offset, WINDOW)).toBe("cool");
  });

  it("calibration preserves the highlight when the clock is fast", () => {
    // The device clock is 30 seconds fast; without calibration, a just-touched file already looks cool.
    const ahead = NOW + 30_000;
    expect(classifyHot(NOW - 1_000, ahead, WINDOW)).toBe("cool");
    const offset = nextClockOffset(null, NOW, ahead);
    expect(classifyHot(NOW - 1_000, ahead + offset, WINDOW)).toBe("fresh");
  });

  it("even a sub-second offset needs calibration near a classification boundary", () => {
    // The device clock is 900 ms slow, and the file age is just beyond the fresh boundary.
    const behind = NOW - 900;
    expect(classifyHot(NOW - 5_400, behind, WINDOW)).toBe("fresh");
    const offset = nextClockOffset(null, NOW, behind);
    expect(classifyHot(NOW - 5_400, behind + offset, WINDOW)).toBe("warm");
  });
});
