import type { Terminal } from "@xterm/xterm";
import type { FitAddon } from "@xterm/addon-fit";

export interface PaneView {
  term: Terminal;
  fit: FitAddon;
}

/// Measure the title cap in display cells so wide glyphs do not overflow.
export const TAB_TITLE_MAX_CELLS = 20;

/// Separate a drag from a header click by requiring pointer travel.
export const PANE_DRAG_THRESHOLD_PX = 4;

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

export function isWide(cp: number): boolean {
  return (
    (cp >= 0x1100 && cp <= 0x115f) ||
    (cp >= 0x2e80 && cp <= 0x303e) ||
    (cp >= 0x3041 && cp <= 0x33ff) ||
    (cp >= 0x3400 && cp <= 0x4dbf) ||
    (cp >= 0x4e00 && cp <= 0x9fff) ||
    (cp >= 0xa000 && cp <= 0xa4cf) ||
    (cp >= 0xac00 && cp <= 0xd7a3) ||
    (cp >= 0xf900 && cp <= 0xfaff) ||
    (cp >= 0xfe30 && cp <= 0xfe4f) ||
    (cp >= 0xff00 && cp <= 0xff60) ||
    (cp >= 0xffe0 && cp <= 0xffe6) ||
    (cp >= 0x1f300 && cp <= 0x1faff) ||
    (cp >= 0x20000 && cp <= 0x3fffd)
  );
}

/// Reserve one display cell for the ellipsis.
export function truncateCells(text: string, max: number): string {
  let width = 0;
  for (const ch of text) width += isWide(ch.codePointAt(0) ?? 0) ? 2 : 1;
  if (width <= max) return text;

  let used = 0;
  let out = "";
  for (const ch of text) {
    const cw = isWide(ch.codePointAt(0) ?? 0) ? 2 : 1;
    if (used + cw > max - 1) break;
    out += ch;
    used += cw;
  }
  return `${out}…`;
}
