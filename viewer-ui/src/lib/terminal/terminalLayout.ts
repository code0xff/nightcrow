import type { Terminal } from "@xterm/xterm";
import type { FitAddon } from "@xterm/addon-fit";
import type { ReplayReplyGate } from "./terminalReplay";

export interface PaneView {
  term: Terminal;
  fit: FitAddon;
  replayGate: ReplayReplyGate;
}

/// Separate a drag from a header click by requiring pointer travel.
export const PANE_DRAG_THRESHOLD_PX = 4;

/// How long a finger rests on a tab before it picks the tab up. A strip of
/// tabs scrolls sideways under a touch, so travel cannot mean "reorder" there;
/// holding still does.
export const PANE_DRAG_HOLD_MS = 350;

export function gcd(a: number, b: number): number {
  while (b) [a, b] = [b, a % b];
  return a;
}

/// Mirror the TUI's balanced row plan, stacking two panes in tall panels.
export function rowPlan(n: number, wide: boolean): number[] {
  switch (n) {
    case 1:
      return [1];
    case 2:
      return wide ? [2] : [1, 1];
    case 3:
      return [2, 1];
    case 4:
      return [2, 2];
    case 5:
      return [3, 2];
    case 6:
      return [3, 3];
    case 7:
      return [4, 3];
    default:
      return [4, 4];
  }
}

export interface CellPlacement {
  row: number;
  colStart: number;
  colSpan: number;
}

export function planLayout(
  n: number,
  wide: boolean,
): { cols: number; rows: number; cells: CellPlacement[] } {
  const plan = rowPlan(n, wide);
  const cols = plan.reduce((acc, c) => (acc * c) / gcd(acc, c), 1);
  const cells: CellPlacement[] = [];
  plan.forEach((count, r) => {
    const span = cols / count;
    for (let k = 0; k < count; k++) {
      cells.push({ row: r + 1, colStart: k * span + 1, colSpan: span });
    }
  });
  return { cols, rows: plan.length, cells };
}
