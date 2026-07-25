import { useCallback, useLayoutEffect, useState } from "react";
import { api } from "./api";

/** Keep accent presets in the TUI's cycle order. */
export const ACCENTS = [
  { name: "yellow", color: "#d9a441" },
  { name: "cyan", color: "#03c4db" },
  { name: "green", color: "#77c47a" },
  { name: "magenta", color: "#dc8fd5" },
  { name: "blue", color: "#87acfd" },
] as const;

/** localStorage caches the server-owned accent for first paint. */
const STORAGE_KEY = "nightcrow.viewer.accent";

function normalize(index: number): number {
  if (!Number.isFinite(index)) return 0;
  const len = ACCENTS.length;
  return ((Math.trunc(index) % len) + len) % len;
}

function loadIndex(): number {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    return raw === null ? 0 : normalize(Number(raw));
  } catch {
    return 0;
  }
}

function storeIndex(index: number) {
  try {
    localStorage.setItem(STORAGE_KEY, String(index));
  } catch {
  }
}

export function useAccent() {
  const [index, setIndex] = useState(loadIndex);

  useLayoutEffect(() => {
    document.documentElement.style.setProperty(
      "--color-accent",
      ACCENTS[index].color,
    );
  }, [index]);

  const cycle = useCallback(() => {
    setIndex((current) => {
      const next = normalize(current + 1);
      storeIndex(next);
      void api.setAccent(next).catch(() => {
      });
      return next;
    });
  }, []);

  /** Apply remote values without echoing them back. */
  const adopt = useCallback((remote: number) => {
    setIndex((current) => {
      const next = normalize(remote);
      if (next === current) return current;
      storeIndex(next);
      return next;
    });
  }, []);

  return {
    accent: ACCENTS[index],
    next: ACCENTS[normalize(index + 1)],
    cycle,
    adopt,
  };
}
