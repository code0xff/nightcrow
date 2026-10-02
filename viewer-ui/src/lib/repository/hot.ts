/** Share recently-touched stages with the TUI's agent indicators. */

export type HotStage = "fresh" | "warm" | "cool";

export const FRESH_MS = 5_000;

export const HOT_TICK_MS = 1_000;

function measureOffset(
  serverNow: number | undefined,
  clientNow: number,
): number {
  if (serverNow === undefined || serverNow <= 0) return 0;
  return serverNow - clientNow;
}

/** Ignore clock-offset jitter smaller than one visible tick. */
export const CLOCK_SKEW_EPSILON_MS = HOT_TICK_MS;

/** Adopt the first offset; update only when a stage can change. */
export function nextClockOffset(
  held: number | null,
  serverNow: number | undefined,
  clientNow: number,
): number {
  const measured = measureOffset(serverNow, clientNow);
  if (held === null) return measured;
  return Math.abs(measured - held) >= CLOCK_SKEW_EPSILON_MS ? measured : held;
}

export function classifyHot(
  mtime: number | undefined,
  now: number,
  windowMs: number,
): HotStage {
  if (mtime === undefined) return "cool";
  const age = Math.max(0, now - mtime);
  if (age >= windowMs) return "cool";
  return age < FRESH_MS ? "fresh" : "warm";
}

export function anyHot(
  mtimes: (number | undefined)[],
  now: number,
  windowMs: number,
): boolean {
  return mtimes.some((m) => classifyHot(m, now, windowMs) !== "cool");
}
