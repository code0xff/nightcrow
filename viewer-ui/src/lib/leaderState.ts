// The leader's state machine: armed or not, and the one two-step command.
//
// The failure this exists to prevent is a leader stuck armed. It waits
// indefinitely for a follow-up (`docs/keybindings.md`), so anything that ends
// the person's train of thought — leaving the window, opening a dialog,
// switching project, a socket reconnect that redraws the page — has to put it
// back to idle, or the next key they type into a pane silently disappears into
// a command. Every one of those is an event here, and there is exactly one
// place that decides what happens next.

import {
  focusPaneNumber,
  type ShortcutActionId,
} from "./shortcutActions";
import type { ShortcutDecision } from "./shortcutKeys";

/** `swapPending` is the second step of `<prefix> s`: waiting for a pane digit. */
export type LeaderState = { armed: false } | { armed: true; swapPending: boolean };

export const IDLE_LEADER: LeaderState = { armed: false };

/**
 * Everything that can move the leader: a classified keystroke, plus the
 * out-of-band events that must disarm it.
 */
export type LeaderEvent =
  | ShortcutDecision
  | { kind: "blur" }
  | { kind: "focusChange" }
  | { kind: "dialogOpen" }
  | { kind: "repoChange" }
  | { kind: "socketReconnect" }
  | { kind: "suppressed" };

export type LeaderEffect =
  | { kind: "none" }
  | { kind: "run"; action: ShortcutActionId }
  | { kind: "swapWith"; pane: number }
  | { kind: "sendLiteralLeader"; data: string };

export interface LeaderStep {
  state: LeaderState;
  effect: LeaderEffect;
}

const NONE: LeaderEffect = { kind: "none" };

function idle(effect: LeaderEffect = NONE): LeaderStep {
  return { state: IDLE_LEADER, effect };
}

/**
 * One transition. Pure: the caller applies `state` and performs `effect`.
 *
 * The pane digits `3`..`9`,`0` are turned into pane numbers 1..8 by the action
 * table alone (`shortcutActions.ts::focusPaneNumber`), so the swap's second step
 * reads a `focus.paneN` action rather than a raw key and the numbering is
 * written down once.
 */
export function reduceLeader(state: LeaderState, event: LeaderEvent): LeaderStep {
  switch (event.kind) {
    // Anything that ends the moment the leader was pressed in disarms it. Doing
    // nothing here is what leaves a leader armed across a dialog or a reconnect.
    case "blur":
    case "focusChange":
    case "dialogOpen":
    case "repoChange":
    case "socketReconnect":
    case "suppressed":
    case "cancel":
      return idle();

    // The key was swallowed by the spent leader and ran nothing: that is the
    // whole point of `consumed`, so there is no effect to report.
    case "consumed":
      return idle();

    case "arm":
      return { state: { armed: true, swapPending: false }, effect: NONE };

    case "literalLeader":
      return idle({ kind: "sendLiteralLeader", data: event.data });

    case "action":
      return reduceAction(state, event.action.id);

    case "ignore":
      return { state, effect: NONE };
  }
}

function reduceAction(state: LeaderState, id: ShortcutActionId): LeaderStep {
  if (state.armed && state.swapPending) {
    const pane = focusPaneNumber(id);
    // A non-pane follow-up abandons the swap rather than running that command:
    // the person asked to swap, and half-executing something else instead would
    // be a surprise with side effects.
    return pane === null ? idle() : idle({ kind: "swapWith", pane });
  }
  // `<prefix> s` is the one command that stays armed: it runs nothing yet and
  // waits for the pane to swap with.
  if (id === "terminal.swapPanePrompt") {
    return { state: { armed: true, swapPending: true }, effect: NONE };
  }
  return idle({ kind: "run", action: id });
}
