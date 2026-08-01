/// Turning a finger dragged across a terminal into wheel notches.
///
/// A wheel is what a terminal already understands. xterm routes one to whatever
/// the pane's program asked for — a mouse report for anything that reads the
/// wheel itself, arrow keys under alternate scroll, the scrollback otherwise —
/// which is the same routing the TUI does by hand in `docs/architecture/terminal.md`.
/// Reproducing that judgement here would be a second copy of it, so the drag
/// only has to produce the event.

/**
 * How far the finger travels per notch, and the travel that makes a drag a
 * scroll rather than a tap.
 *
 * One number for both because the first notch is what starts the scroll. Fifty
 * pixels also keeps xterm from damping the delta: it scales anything under that
 * down by 0.3, taking a trackpad's small steps for what they are, and a drag fed
 * to it in single-pixel moves would crawl at a third of the finger's speed. At
 * this size a notch is a few rows and the text keeps up with the finger.
 */
export const WHEEL_STEP_PX = 50;

export interface TouchScroll {
  /** Where the finger was on the last move. */
  lastY: number;
  /** Travel not yet spent on a notch, in wheel pixels (positive scrolls down). */
  pending: number;
  /** Whether this drag has become a scroll, and so is no longer a tap. */
  scrolling: boolean;
}

export function beginTouchScroll(y: number): TouchScroll {
  return { lastY: y, pending: 0, scrolling: false };
}

/**
 * Move the drag to `y`, and say how far the wheel should turn for it.
 *
 * `deltaY` follows the wheel's sign, not the finger's: dragging up reveals what
 * comes next, which is a wheel scrolled down. Travel accumulates rather than
 * being spent as it arrives, so a finger that wanders back to where it started
 * has scrolled nothing.
 */
export function advanceTouchScroll(
  state: TouchScroll,
  y: number,
): { next: TouchScroll; deltaY: number } {
  const pending = state.pending + (state.lastY - y);
  if (Math.abs(pending) < WHEEL_STEP_PX) {
    return { next: { ...state, lastY: y, pending }, deltaY: 0 };
  }
  return {
    next: { lastY: y, pending: 0, scrolling: true },
    deltaY: pending,
  };
}
