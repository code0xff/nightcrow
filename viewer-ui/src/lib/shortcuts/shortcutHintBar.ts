// The TUI's hint bar, said with the web registry.
//
// The TUI keeps one line under everything that names the leader and what it
// does next (`src/ui/hint_text.rs`); this builds the same line for the browser
// from `SHORTCUT_ACTIONS`, so what is printed is what is bound. A command that
// cannot run here is left out rather than dimmed — a hint for a no-op key would
// lie, which is the TUI's rule too. Pure, so the line can be tested without a
// page around it; the component only prints what comes back.

import { formatChord, type ChordSpec } from "./leaderChord";
import type { LeaderState } from "./leaderState";
import {
  SHORTCUT_ACTIONS,
  focusPaneNumber,
  type ShortcutAction,
  type ShortcutActionId,
} from "./shortcutActions";

/** What a click on a segment does: run a command, or arm the leader as the
 *  TUI's `<prefix>` chip does. Null for a segment that only informs. */
export type HintClick =
  | { kind: "run"; action: ShortcutActionId }
  | { kind: "arm" };

export interface HintSegment {
  /** The keys as a person reads them: `Ctrl+F t`, `t`, `esc`. */
  keys: string;
  label: string;
  click: HintClick | null;
}

export interface HintLine {
  /** The chip in front of the line while a step is pending, as the TUI's
   *  ` PREFIX ` and ` SWAP `. */
  chip: "PREFIX" | "SWAP" | null;
  segments: HintSegment[];
}

/** The commands the idle line advertises, in the TUI's order: the leader and
 *  the handful reached for first. The armed line lists every command. */
const IDLE_ACTIONS: readonly ShortcutActionId[] = [
  "terminal.newPane",
  "terminal.closePane",
  "view.toggleMaximize",
  "project.openDialog",
  "help.shortcuts",
];

/** The pane digits as one segment. Eight entries would be the whole line, and
 *  a click has no digit to offer, so it informs only. */
const PANE_DIGITS: HintSegment = {
  keys: "3-9,0",
  label: "pane 1-8",
  click: null,
};
const SWAP_TARGET: HintSegment = {
  keys: "3-9,0",
  label: "swap active pane with this pane",
  click: null,
};
const CANCEL: HintSegment = { keys: "esc", label: "cancel", click: null };

export function hintLine(
  state: LeaderState,
  leader: ChordSpec | null,
  isAvailable: (id: ShortcutActionId) => boolean,
): HintLine {
  if (state.armed && state.swapPending) {
    return { chip: "SWAP", segments: [SWAP_TARGET, CANCEL] };
  }
  if (state.armed) return { chip: "PREFIX", segments: armedSegments(isAvailable) };
  if (!leader) return { chip: null, segments: leaderlessSegments(isAvailable) };
  return { chip: null, segments: idleSegments(leader, isAvailable) };
}

function idleSegments(
  leader: ChordSpec,
  isAvailable: (id: ShortcutActionId) => boolean,
): HintSegment[] {
  const chord = formatChord(leader);
  const segments: HintSegment[] = [
    { keys: chord, label: "leader", click: { kind: "arm" } },
  ];
  for (const action of SHORTCUT_ACTIONS) {
    if (!IDLE_ACTIONS.includes(action.id) || !isAvailable(action.id)) continue;
    segments.push(run(action, `${chord} ${action.leader}`));
  }
  return segments;
}

/** Every leader command that can run, with the pane digits folded into one
 *  segment where the first of them sits, then the way out. */
function armedSegments(
  isAvailable: (id: ShortcutActionId) => boolean,
): HintSegment[] {
  const segments: HintSegment[] = [];
  let digitsShown = false;
  for (const action of SHORTCUT_ACTIONS) {
    if (action.leader === undefined || !isAvailable(action.id)) continue;
    if (focusPaneNumber(action.id) !== null) {
      if (!digitsShown) segments.push(PANE_DIGITS);
      digitsShown = true;
      continue;
    }
    segments.push(run(action, action.leader));
  }
  segments.push(CANCEL);
  return segments;
}

/** With the leader switched off only the standalone chords are keys, and the
 *  sheet is where the leader comes back — so that is what the line offers. */
function leaderlessSegments(
  isAvailable: (id: ShortcutActionId) => boolean,
): HintSegment[] {
  const segments: HintSegment[] = [
    {
      keys: "leader",
      label: "switched off",
      click: { kind: "run", action: "help.shortcuts" },
    },
  ];
  for (const action of SHORTCUT_ACTIONS) {
    if (action.chord === undefined || !isAvailable(action.id)) continue;
    segments.push(run(action, action.chord));
  }
  return segments;
}

function run(action: ShortcutAction, keys: string): HintSegment {
  return { keys, label: action.hint, click: { kind: "run", action: action.id } };
}
