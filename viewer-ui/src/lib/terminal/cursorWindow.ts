/// Which part of a terminal a pane shows when the pane is shorter than it.
///
/// That happens on purpose while a soft keyboard is up: the panes keep their
/// grid rather than refit (`usePaneSizes`), and the cell crops. Cropping to the
/// bottom assumes the prompt is the last row, which holds once a screen has
/// filled and fails on a fresh terminal — its prompt is the top row, and a
/// bottom crop shows the empty rows under it while the line being typed sits
/// out of sight behind the keyboard. The cursor is where the typing is, so the
/// window is placed around it.

export interface CursorWindow {
  /** The terminal's full rendered height. */
  termHeight: number;
  /** The height the pane has to show it in. */
  bodyHeight: number;
  rows: number;
  /** The cursor's row in the visible screen, 0-based. */
  cursorRow: number;
  /** Whether the viewport is at the live screen rather than scrolled back. */
  atLive: boolean;
}

/**
 * How far below the pane's bottom edge the terminal's bottom should sit, in
 * pixels — 0 is the plain bottom crop.
 *
 * The window is as low as it can be while still showing the cursor's row, so
 * a full screen crops as it always did and only a cursor high in the grid
 * moves it up. Scrolled back, the cursor is off screen by the person's choice,
 * and the crop is left alone.
 */
export function cursorWindowShift({
  termHeight,
  bodyHeight,
  rows,
  cursorRow,
  atLive,
}: CursorWindow): number {
  const overflow = termHeight - bodyHeight;
  if (overflow <= 0 || rows <= 0 || !atLive) return 0;
  const rowHeight = termHeight / rows;
  const cursorBottom = (Math.min(Math.max(cursorRow, 0), rows - 1) + 1) * rowHeight;
  const top = Math.min(Math.max(cursorBottom - bodyHeight, 0), overflow);
  return Math.round(overflow - top);
}
