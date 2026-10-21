/**
 * Script-driven motion for the terminal panel, where a CSS entrance is not
 * enough: a pane closing has to finish fading before the close goes out, and
 * a layout change replays on an element that stays mounted.
 *
 * Only opacity and transform move. A terminal's box never animates — every
 * size it passed through would be a fit, and a fit that settles is a PTY
 * resize (SIGWINCH and a full repaint, on every attached screen).
 */

export const CLOSE_FADE_MS = 120;
const LAYOUT_FADE_MS = 140;
const EASE = "cubic-bezier(0.2, 0, 0, 1)";

export function motionReduced(): boolean {
  return (
    typeof window === "undefined" ||
    typeof window.matchMedia !== "function" ||
    window.matchMedia("(prefers-reduced-motion: reduce)").matches
  );
}

function canAnimate(el: Element | null): el is HTMLElement {
  return el instanceof HTMLElement && typeof el.animate === "function" && !motionReduced();
}

/** Fade `el` out, then run `done` — at once when there is nothing to animate.
 *  `done` runs exactly once, also if the animation is cancelled. */
export function fadeOutThen(el: Element | null, done: () => void): void {
  if (!canAnimate(el)) {
    done();
    return;
  }
  let ran = false;
  const finish = () => {
    if (ran) return;
    ran = true;
    done();
  };
  const animation = el.animate(
    [
      { opacity: 1, transform: "scale(1)" },
      { opacity: 0, transform: "scale(0.98)" },
    ],
    { duration: CLOSE_FADE_MS, easing: EASE, fill: "forwards" },
  );
  animation.onfinish = finish;
  animation.oncancel = finish;
}

/** Ease `el` back in after its layout jumped to a new arrangement. */
export function fadeInAfterLayout(el: Element | null): void {
  if (!canAnimate(el)) return;
  el.animate([{ opacity: 0.35 }, { opacity: 1 }], { duration: LAYOUT_FADE_MS, easing: EASE });
}
