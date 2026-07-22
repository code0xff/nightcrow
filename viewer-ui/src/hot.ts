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

/** Bucket one mtime against `now`. Negative ages — a clock skewed between the
 *  server that stat'd the file and the browser dating it — saturate to `fresh`,
 *  the same conservative "just touched" choice the TUI makes. */
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
