/** The "recently touched" highlight for the status file list, mirroring the
 *  TUI's `classify_hot` (src/ui/file_list.rs) stage for stage. The server sends
 *  each changed file's mtime; the window comes from the same `agent_indicator`
 *  config the TUI reads, so both surfaces fade on the same schedule. */

export type HotStage = "fresh" | "warm" | "cool";

/** Below this age a file is "just touched" and gets the loudest treatment. Well
 *  above filesystem mtime granularity (1s on older filesystems) so the
 *  fresh→warm step stays easy to see. Matches the TUI's threshold. */
export const FRESH_MS = 5_000;

/** How often a hot list has to re-render to fade on time. The stages are
 *  seconds wide, so a second is fine-grained enough. */
export const HOT_TICK_MS = 1_000;

/** How far ahead of this device the server's clock runs, in milliseconds. Add it
 *  to `Date.now()` to get an instant comparable with the `mtime`s the server
 *  sends, which it measured against its own clock.
 *
 *  The correction matters because the default hot window is 15s: a device a
 *  handful of seconds slow would over-highlight for that long, and one 15s fast
 *  would never light up at all. Undefined `serverNow` (a server too old to send
 *  it) means no correction — the local clock is the best guess left.
 *
 *  One-way network latency is folded into the result, which is fine here: it is
 *  tens of milliseconds against stages measured in seconds. */
export function clockOffset(
  serverNow: number | undefined,
  clientNow: number,
): number {
  if (serverNow === undefined || serverNow <= 0) return 0;
  return serverNow - clientNow;
}

/** Smallest offset change worth adopting. Each poll measures the offset afresh,
 *  so network jitter moves it by tens of milliseconds even on a device whose
 *  clock never drifts; adopting every reading would restart the fade ticker
 *  every poll for a correction no stage is wide enough to show. One tick is the
 *  natural floor — a shift the list cannot render is a shift not worth taking. */
export const CLOCK_SKEW_EPSILON_MS = HOT_TICK_MS;

/** Bucket one mtime against `now`, which callers derive from
 *  [`clockOffset`] so both sides of the subtraction share a clock.
 *
 *  A negative age still saturates to `fresh`: with the offset applied what
 *  remains is sub-second ordering between the `stat` and the timestamp, exactly
 *  the case the TUI resolves the same conservative way. */
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

/** Whether any file is still inside the window — i.e. whether the list has an
 *  animation left to run and needs the per-second tick. */
export function anyHot(
  mtimes: (number | undefined)[],
  now: number,
  windowMs: number,
): boolean {
  return mtimes.some((m) => classifyHot(m, now, windowMs) !== "cool");
}
