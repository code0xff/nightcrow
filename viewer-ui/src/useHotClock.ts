import { useEffect, useState } from "react";
import { anyHot, HOT_TICK_MS, type HotStage } from "./hot";
import type { ChangedFile } from "./api";

/** Keep change-kind colors stable as highlights fade. */
export const HOT_CLASS: Record<HotStage, string> = {
  fresh: "text-accent font-bold",
  warm: "text-accent",
  cool: "",
};

/** Avoid intervals when no file can remain hot. */
export function useHotClock(
  files: ChangedFile[] | undefined,
  windowMs: number,
  offsetMs: number,
): number {
  const [now, setNow] = useState(() => Date.now() + offsetMs);
  useEffect(() => {
    if (windowMs <= 0 || !files) return;
    const mtimes = files.map((f) => f.mtime);
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
