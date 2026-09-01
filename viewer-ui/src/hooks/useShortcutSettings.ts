import { useCallback, useState } from "react";
import {
  DEFAULT_LEADER,
  formatChord,
  leaderConflict,
  parseChord,
  type ChordSpec,
} from "../lib/leaderChord";

// The leader is a per-browser preference, so it stays in the browser.
//
// Everything shared — the accent, the panel split, which project is in front —
// lives in the session and is polled, because two clients must agree on it. This
// is the opposite: `Ctrl+F` is the browser's Find, so which chord is reachable
// depends on the browser and the keyboard in front of the person, and a phone
// and a desktop looking at the same session want different answers. Sending it
// to `viewer.json` would make one of them wrong.
//
// `localStorage`, not `sessionStorage`: a rebinding is meant to outlive the tab,
// unlike which pane had the keyboard (`lib/lastPane.ts`).

const KEY = "nightcrow.shortcut.leader";

/** Stored as `{ leader: "Ctrl+F" }`, with `null` for "switched off" — a
 *  sentinel string would be indistinguishable from a chord named that. */
interface Stored {
  leader: string | null;
}

export interface ShortcutSettings {
  /** The configured leader, or null when the user has switched it off. */
  leader: ChordSpec | null;
  /** The chord in display form, empty while it is switched off. */
  leaderText: string;
  /** A known browser or OS collision, for the help sheet to show. The default
   *  `Ctrl+F` reports the Find collision, which is intended. */
  conflict: string | null;
  /** Accept a chord written as `Ctrl+Shift+ArrowLeft`. False leaves the current
   *  leader alone: unparsable text is a typo, not a request to switch off. */
  setLeader: (text: string) => boolean;
  disable: () => void;
  reset: () => void;
}

export function useShortcutSettings(): ShortcutSettings {
  // Read once per mount. Nothing else writes the key, and a second tab changing
  // its own leader is not news this page has to follow.
  const [leader, setLeaderState] = useState<ChordSpec | null>(readLeader);

  const persist = useCallback((next: ChordSpec | null) => {
    setLeaderState(next);
    write(next === null ? null : formatChord(next));
  }, []);

  const setLeader = useCallback(
    (text: string) => {
      const spec = parseChord(text);
      if (!spec) return false;
      persist(spec);
      return true;
    },
    [persist],
  );

  const disable = useCallback(() => persist(null), [persist]);
  const reset = useCallback(() => persist(DEFAULT_LEADER), [persist]);

  return {
    leader,
    leaderText: leader ? formatChord(leader) : "",
    conflict: leader ? leaderConflict(leader) : null,
    setLeader,
    disable,
    reset,
  };
}

/**
 * Storage is a boundary: another version of this page, or a person with the
 * developer tools open, can have written anything under the key. Anything that
 * is not a chord this build understands falls back to the default rather than
 * leaving the page with no shortcuts and no way to say why.
 */
function readLeader(): ChordSpec | null {
  const stored = read();
  if (!stored) return DEFAULT_LEADER;
  if (stored.leader === null) return null;
  return parseChord(stored.leader) ?? DEFAULT_LEADER;
}

function read(): Stored | null {
  try {
    const raw = localStorage.getItem(KEY);
    if (!raw) return null;
    const parsed: unknown = JSON.parse(raw);
    if (!parsed || typeof parsed !== "object" || Array.isArray(parsed)) {
      return null;
    }
    const value = (parsed as { leader?: unknown }).leader;
    if (value === null) return { leader: null };
    return typeof value === "string" ? { leader: value } : null;
  } catch {
    // Storage can be disabled outright, and `JSON.parse` throws on anything
    // that is not JSON at all. Either way the page works with the default.
    return null;
  }
}

function write(leader: string | null): void {
  try {
    localStorage.setItem(KEY, JSON.stringify({ leader } satisfies Stored));
  } catch {
    // A page that cannot store this is the page as it was before this existed:
    // the leader holds for the tab and is forgotten on reload.
  }
}
