import { useEffect, useState } from "react";
import { anyHot, HOT_TICK_MS, type HotStage } from "./hot";
import type { ChangedFile } from "./api";

/** Recency styling for one status row, mirroring the TUI: the status letters
 *  keep their change colour at every stage — the change kind stays readable —
 *  and only the path carries the highlight, so a row does not shift as it
 *  fades. */
export const HOT_CLASS: Record<HotStage, string> = {
  fresh: "text-accent font-bold",
  warm: "text-accent",
  cool: "",
};

/** The clock the recently-touched highlight is dated against.
 *
 *  A file cools with time rather than with any event, so the list has to
 *  re-render on its own to fade. The ticking is bounded on both ends: it starts
 *  only when a snapshot actually contains a hot file, and stops itself once the
 *  last one cools — an idle repository re-renders nothing. Every snapshot is
 *  still dated on arrival, ticker or not, so a stopped clock never judges one.
 *
 *  `windowMs <= 0` (the server's indicator turned off, or its config not yet
 *  loaded) never ticks; `classifyHot` reads everything as cool at that window. */
export function useHotClock(
  files: ChangedFile[] | undefined,
  windowMs: number,
  offsetMs: number,
): number {
  // Every reading is shifted onto the server's clock, because that is the clock
  // the mtimes it is compared against were measured on. `offsetMs` is a
  // dependency so a poll that refines it restarts the tick on the corrected
  // clock rather than finishing the current fade on the old one.
  const [now, setNow] = useState(() => Date.now() + offsetMs);
  useEffect(() => {
    if (windowMs <= 0 || !files) return;
    const mtimes = files.map((f) => f.mtime);
    // Date the snapshot before deciding whether it needs a ticker, not after.
    // `now` stops advancing when the last file cools, so a snapshot arriving
    // long afterwards — the tab left open, or another repository selected —
    // would otherwise be measured against whenever the ticker last stopped, and
    // a file touched around that moment would read as freshly touched forever.
    const start = Date.now() + offsetMs;
    setNow(start);
    if (!anyHot(mtimes, start, windowMs)) return;
    const id = setInterval(() => {
      const tick = Date.now() + offsetMs;
      setNow(tick);
      if (!anyHot(mtimes, tick, windowMs)) clearInterval(id);
    }, HOT_TICK_MS);
    return () => clearInterval(id);
  }, [files, windowMs, offsetMs]);
  return now;
}