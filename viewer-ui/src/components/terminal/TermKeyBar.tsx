import { useEffect } from "react";
import { TERM_KEY_BAR, type TermKey } from "../../lib/termKeys";
import type { CtrlLatch } from "../../hooks/terminal/useCtrlLatch";

const KEY_BUTTON =
  "flex min-h-9 min-w-9 shrink-0 items-center justify-center rounded-sm border px-2 text-xs active:bg-ink-700 active:text-accent";
const KEY_IDLE = "border-ink-700 bg-ink-850 text-ink-200";
/** Armed Ctrl. Held down is a state, not a press, so it stays lit until it is
 *  spent — otherwise the only sign of a latch nobody meant to arm is the next
 *  character going missing. */
const KEY_ARMED = "border-accent bg-ink-700 text-accent";

/**
 * The keys a soft keyboard does not have, for the panes on a touch device.
 *
 * Shown at every width — a tablet is as wide as a desktop and types like a
 * phone, so the breakpoint was never the question (`defaultKeyBarShown`).
 * Whether it is here at all is the panel toolbar's toggle.
 *
 * `onPointerDown` is prevented so the tap does not blur the terminal: losing
 * focus would send the key to nothing.
 */
export function TermKeyBar({
  onKey,
  ctrl,
  onArm,
}: {
  onKey: (key: TermKey) => void;
  ctrl: CtrlLatch;
  /** Put the keyboard in the pane. The other keys go out over the socket and
   *  need no focus, but the latch is spent by the next character *typed*, so
   *  tapping it has to leave somewhere to type. */
  onArm: () => void;
}) {
  // The latch outlives this bar — it belongs to the panel — and armed with no
  // button to say so, it would modify a character typed long after, for no
  // visible reason. So it does not survive the bar being taken away: hidden
  // from the toolbar, or the last pane closing. A panel merely CSS-hidden (the
  // phone showing another view) keeps both, which is the point — they come back
  // together, lit.
  useEffect(() => ctrl.clear, [ctrl.clear]);

  return (
    <div className="flex shrink-0 items-stretch gap-1 overflow-x-auto border-t border-ink-700 bg-ink-900 px-1 py-1">
      {TERM_KEY_BAR.map((item) =>
        item.kind === "ctrl" ? (
          <button
            key="ctrl"
            onPointerDown={(event) => event.preventDefault()}
            // On the way up only. Disarming has nothing waiting to be typed, so
            // taking the keyboard for it would be the bar interrupting whatever
            // the person moved on to — the panel's own rule about text entry
            // outside it (`focusIsTakeable`). What the tap did is the toggle's
            // answer, not `ctrl.armed`, which is a render behind it.
            onClick={() => {
              if (ctrl.toggle()) onArm();
            }}
            aria-pressed={ctrl.armed}
            aria-label={item.aria}
            className={`${KEY_BUTTON} ${ctrl.armed ? KEY_ARMED : KEY_IDLE}`}
          >
            {item.label}
          </button>
        ) : (
          <button
            key={item.key}
            onPointerDown={(event) => event.preventDefault()}
            // A latch left armed is spent here rather than carried past this
            // tap: these keys send their own bytes and have no room for a Ctrl,
            // and one kept for later would attach itself to whatever is typed
            // next, long after the person forgot they armed it.
            onClick={() => {
              ctrl.clear();
              onKey(item.key);
            }}
            aria-label={item.aria}
            className={`${KEY_BUTTON} ${KEY_IDLE}`}
          >
            {item.label}
          </button>
        ),
      )}
    </div>
  );
}
