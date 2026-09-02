import { useCallback, useEffect, useRef, useState } from "react";
import type { ChordSpec } from "../lib/leaderChord";
import {
  IDLE_LEADER,
  reduceLeader,
  type LeaderEffect,
  type LeaderEvent,
  type LeaderState,
} from "../lib/leaderState";
import { classifyShortcutKey } from "../lib/shortcutKeys";
import { describeShortcutTarget } from "../lib/shortcutDom";
import { isTextEntryTarget, shortcutsSuppressed } from "../lib/shortcutTarget";
import { useGlobalKeydown } from "./useGlobalKeydown";
import { useShortcutIntents, type ShortcutIntents } from "./shortcutIntents";

// The one keyboard decision point.
//
// Every page-level key goes through here: the leader and its follow-ups, and
// the standalone chords in the registry (`Ctrl+Shift+Arrow` project cycling).
// There is deliberately no second listener anywhere — two of them cannot agree
// on whether a key was claimed, and the one that loses either eats a keystroke
// the pane needed or lets a command leak into the shell as an escape sequence.
//
// Nothing judgemental happens here. `classifyShortcutKey` decides what a key
// means, `reduceLeader` decides what that does to the leader, and this carries
// out the result: consume the event, or leave it completely untouched so xterm
// encodes it and the PTY receives exactly what the person typed.

export interface UseShortcutsArgs {
  /** Off before the first authenticated poll: the login screen owns its own
   *  keyboard, and none of the commands have anything to act on. */
  enabled: boolean;
  leader: ChordSpec | null;
  /** A modal surface has the keyboard (the folder picker, the shortcut sheet). */
  dialogOpen: boolean;
  /** The project on screen, watched only as a disarm signal. */
  repo: string | null;
}

/** What the engine lets the page see and do: the leader's state for the hint
 *  line to print, and the two moves a click on that line makes. */
export interface ShortcutEngine {
  state: LeaderState;
  /** Arm the leader, as the TUI's `<prefix>` chip does when clicked. */
  arm: () => void;
  /** Put the leader back to idle, for a hint that ran its command by click. */
  disarm: () => void;
}

export function useShortcuts({
  enabled,
  leader,
  dialogOpen,
  repo,
}: UseShortcutsArgs): ShortcutEngine {
  const intents = useShortcutIntents();
  // The ref is what a keystroke reads and what the reducer writes, so a leader
  // and its follow-up can never straddle a render and disagree. The state is a
  // mirror of it for the hint line, which is the one thing that renders the
  // armed leader; nothing decides from the mirror.
  const state = useRef<LeaderState>(IDLE_LEADER);
  const [shown, setShown] = useState<LeaderState>(IDLE_LEADER);
  // The bus through a ref, so `dispatch` has one identity for the hook's whole
  // life. The bus hands out a new object whenever the set of registered actions
  // changes — a pane opening does that — and a `dispatch` that changed with it
  // would re-run the effects below, disarming a leader for an unrelated reason.
  const busRef = useRef(intents);
  busRef.current = intents;

  const dispatch = useCallback((event: LeaderEvent) => {
    const step = reduceLeader(state.current, event);
    state.current = step.state;
    setShown(step.state);
    const bus = busRef.current;
    if (bus) perform(step.effect, bus);
  }, []);

  const onKeyDown = useCallback(
    (event: KeyboardEvent) => {
      const target = describeShortcutTarget(event.target);
      const suppressed = shortcutsSuppressed({
        target,
        dialogOpen,
        // Only `isComposing` here. The other two IME tells — `keyCode` 229 and a
        // `Process`/`Unidentified` key — are `classifyShortcutKey`'s, and
        // repeating them would be a second copy of that rule free to drift.
        composing: event.isComposing === true,
      });
      const decision = classifyShortcutKey(event, {
        leader,
        armed: state.current.armed,
        suppressed,
      });
      // Suppression is also a reason to disarm, which the pure classifier cannot
      // do — see its doc comment. Without this a leader armed a moment before a
      // dialog opened would still be armed after it closed.
      if (suppressed) {
        dispatch({ kind: "suppressed" });
        return false;
      }
      dispatch(decision);
      // `arm`, `cancel`, `consumed`, `action` and `literalLeader` are all keys
      // the page has claimed; only `ignore` belongs to the pane and the browser.
      return decision.kind !== "ignore";
    },
    [dialogOpen, leader, dispatch],
  );

  useGlobalKeydown(onKeyDown, enabled && intents !== null);

  // Leaving the window, and the keyboard moving into something that owns it.
  // Both are the person's attention going elsewhere with a leader still armed.
  useEffect(() => {
    if (!enabled) return;
    const onBlur = () => dispatch({ kind: "blur" });
    const onFocusIn = (event: FocusEvent) => {
      const target = describeShortcutTarget(event.target);
      if (isTextEntryTarget(target) || target?.inDialog === true) {
        dispatch({ kind: "focusChange" });
      }
    };
    window.addEventListener("blur", onBlur);
    document.addEventListener("focusin", onFocusIn, { capture: true });
    return () => {
      window.removeEventListener("blur", onBlur);
      document.removeEventListener("focusin", onFocusIn, { capture: true });
    };
  }, [enabled, dispatch]);

  useEffect(() => {
    if (dialogOpen) dispatch({ kind: "dialogOpen" });
  }, [dialogOpen, dispatch]);

  // The keyboard being taken away altogether: the session expired mid-sequence
  // and the login screen has it. This hook stays mounted through that, so the
  // armed state outlives the listener unless it is cleared here — and signing
  // back in would spend the first key typed into a pane on a leader pressed
  // before the interruption.
  useEffect(() => {
    if (!enabled) dispatch({ kind: "disabled" });
  }, [enabled, dispatch]);

  // A project switch, which is also the terminal socket's own signal: the socket
  // effect is keyed on the repository (`useTerminalSocket`), so a switch tears
  // every pane down and hands out ids that mean something else.
  //
  // Compared against what was last seen rather than run on mount, so arming the
  // leader in the first render after a switch is not undone by this effect.
  const seenRepo = useRef(repo);
  useEffect(() => {
    if (seenRepo.current === repo) return;
    seenRepo.current = repo;
    dispatch({ kind: "repoChange" });
  }, [repo, dispatch]);

  // A reconnect inside the same project. Nothing at this level can see it — the
  // socket lives in the panel — so the panel reports it over the bus.
  useEffect(() => {
    if (!intents) return;
    return intents.onDisarm(() => dispatch({ kind: "socketReconnect" }));
  }, [intents, dispatch]);

  // A leader that has just been rebound or switched off cannot have a follow-up
  // pending: the chord that armed it is no longer the leader.
  useEffect(() => {
    dispatch({ kind: "cancel" });
  }, [leader, dispatch]);

  const arm = useCallback(() => dispatch({ kind: "arm" }), [dispatch]);
  const disarm = useCallback(() => dispatch({ kind: "cancel" }), [dispatch]);
  return { state: shown, arm, disarm };
}

function perform(effect: LeaderEffect, intents: ShortcutIntents): void {
  switch (effect.kind) {
    case "none":
      return;
    case "run":
      intents.runAction(effect.action);
      return;
    case "swapWith":
      intents.swapPanes(effect.pane);
      return;
    case "sendLiteralLeader":
      // Empty for a leader chord a terminal has no encoding for. Sending it
      // would put a stray byte in the shell instead of nothing.
      if (effect.data) intents.sendLiteralLeader(effect.data);
      return;
  }
}
