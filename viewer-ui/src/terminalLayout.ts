import type { Terminal } from "@xterm/xterm";
import type { FitAddon } from "@xterm/addon-fit";

export interface PaneView {
  term: Terminal;
  fit: FitAddon;
}

/// Pane titles are capped by display width (not character count) so a title of
/// wide CJK glyphs cannot overflow its cell header; the full title stays
/// reachable through the tooltip. Matches the viewer's label convention.
export const TAB_TITLE_MAX_CELLS = 20;

/// Pointer travel before a header press becomes a pane drag rather than a click
/// that just focuses the pane. Mirrors the sidebar divider's small dead zone.
export const PANE_DRAG_THRESHOLD_PX = 4;

export function gcd(a: number, b: number): number {
  while (b) [a, b] = [b, a % b];
  return a;
}

/// Columns per row for `n` panes, mirroring the TUI's `grid_row_plan`
/// (src/ui/terminal_tab.rs): a balanced grid, with the two-pane case flipping to
/// stacked when the panel is taller than it is wide.
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
      return [4, 4]; // 8 (the per-repo cap); also a sane fallback beyond it
  }
}

export interface CellPlacement {
  row: number;
  colStart: number;
  colSpan: number;
}

/// Flatten `rowPlan` into a CSS-grid placement per pane. Rows can hold different
/// column counts (e.g. 3 = [2,1]); a shared column count (the LCM of the rows'
/// counts) lets each cell span evenly so every row fills the width.
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

/// True for code points that occupy two terminal cells. An approximation of the
/// common East Asian wide / fullwidth ranges — enough to keep CJK titles from
/// overflowing without pulling in a full Unicode width table.
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

/// Truncate `text` to at most `max` display cells, appending an ellipsis (which
/// costs one cell) when anything was dropped.
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