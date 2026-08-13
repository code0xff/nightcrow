// The Ctrl the bar cannot spell out. Only a handful of combinations fit on it,
// and a shell uses more than that (`^A`, `^E`, `^K`, `^W`, `^P`…), so this one
// arms and the next character sent from the soft keyboard leaves as its control
// byte.
//
// It reads the character, not the keypress. A soft keyboard is under no
// obligation to say which key was struck — iOS and Android report
// `Unidentified` and keyCode 229 for much of what an IME sends — so a keydown
// handler would have nothing to modify. By the time the same input reaches
// `term.onData` it is a character, whatever produced it.

import { useCallback, useRef, useState } from "react";
import { ctrlLatchStep } from "../../lib/termKeys";

export interface CtrlLatch {
  /** Whether the next character is being modified — the button's pressed state. */
  armed: boolean;
  /** Flips the latch and says what it became. Returned rather than read back
   *  off `armed`, which is a render old — two taps inside one batch would both
   *  see the same snapshot and neither would know which of them armed it. */
  toggle: () => boolean;
  clear: () => void;
  /**
   * The bytes to send for `typed`, disarming the latch if it was armed.
   *
   * Stable across renders: the input handler that calls it is created once per
   * pane, and so holds whichever one existed when its xterm was opened.
   */
  consume: (typed: string) => string;
}

export function useCtrlLatch(): CtrlLatch {
  const [armed, setArmed] = useState(false);
  // The state is what renders the button; the ref is what `consume` reads, which
  // is called from a closure older than any of these renders.
  const armedRef = useRef(false);
  const set = useCallback((next: boolean) => {
    armedRef.current = next;
    setArmed(next);
  }, []);

  // What is decided here is only when to re-render; which bytes go out, and
  // whether the latch survives them, is `ctrlLatchStep`.
  const consume = useCallback(
    (typed: string) => {
      const step = ctrlLatchStep(armedRef.current, typed);
      if (step.armed !== armedRef.current) set(step.armed);
      return step.data;
    },
    [set],
  );

  return {
    armed,
    toggle: useCallback(() => {
      const next = !armedRef.current;
      set(next);
      return next;
    }, [set]),
    clear: useCallback(() => set(false), [set]),
    consume,
  };
}
