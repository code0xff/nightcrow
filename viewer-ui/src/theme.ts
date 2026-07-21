import { useCallback, useLayoutEffect, useState } from "react";

/**
 * Accent presets, in the TUI's cycle order (`config.rs` `Accent::ALL`), so
 * `<prefix> p` there and the header swatch here walk the same sequence.
 *
 * The TUI names ratatui palette colours, which resolve to whatever the user's
 * terminal theme assigns. The browser has no such palette, so these are fixed
 * hexes derived from the existing amber: `#d9a441` is OKLCH L=0.751 C=0.130
 * h=79.8, and each sibling holds that lightness and chroma while rotating the
 * hue. Matching L keeps the accent equally readable against `--color-ink-*` in
 * every preset, which picking hexes by eye does not.
 */
export const ACCENTS = [
  { name: "yellow", color: "#d9a441" },
  { name: "cyan", color: "#03c4db" },
  // Shares a hue family with `--color-added` (#4ba36b); they separate on
  // lightness rather than hue. The TUI's Accent::Green sits beside its own
  // green status colour the same way, so this preset is no worse there.
  { name: "green", color: "#77c47a" },
  { name: "magenta", color: "#dc8fd5" },
  // Chroma trimmed to 0.125 — 0.130 at this hue falls outside sRGB.
  { name: "blue", color: "#87acfd" },
] as const;

const STORAGE_KEY = "nightcrow.viewer.accent";

/**
 * The accent is one setting for the whole viewer, not one per project.
 * Repository ids are only stable for the lifetime of the process
 * (`viewer/catalog.rs`), so keying this by repo would silently drop the
 * preference on every restart.
 */
function normalize(index: number): number {
  if (!Number.isFinite(index)) return 0;
  const len = ACCENTS.length;
  // Euclidean remainder: a corrupt negative index still lands in range.
  return ((Math.trunc(index) % len) + len) % len;
}

function loadIndex(): number {
  // localStorage throws rather than returning null when storage is blocked
  // (Safari private browsing, site data disabled); an unstyled accent is a far
  // better outcome than a blank page.
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
    // Preference is lost on reload; the session still renders correctly.
  }
}

/**
 * Current accent plus a cycle step, mirroring the TUI's `cycle_accent`.
 *
 * Applied by overriding `--color-accent` on the root element: Tailwind compiles
 * every accent utility to `var(--color-accent)`, so one property recolours all
 * of them without a rebuild. The CSP forbids inline scripts (`script-src
 * 'self'`), so this cannot run before the bundle does — a hard reload paints
 * the default amber for the moment before React mounts.
 */
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
      return next;
    });
  }, []);

  return { accent: ACCENTS[index], next: ACCENTS[normalize(index + 1)], cycle };
}
