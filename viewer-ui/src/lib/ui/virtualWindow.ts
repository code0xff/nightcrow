export const VIRTUAL_ROW_PX = 20;
export const VIRTUAL_OVERSCAN = 12;
export const VIRTUAL_THRESHOLD = 200;

export interface ScrollViewport {
  scrollTop: number;
  height: number;
}

export function virtualWindow(
  count: number,
  scrollTop: number,
  height: number,
  rowHeight = VIRTUAL_ROW_PX,
  overscan = VIRTUAL_OVERSCAN,
) {
  const visibleRows = Math.max(1, Math.ceil(height / rowHeight));
  const visibleStart = Math.min(
    Math.floor(Math.max(0, scrollTop) / rowHeight),
    Math.max(0, count - visibleRows),
  );
  const visibleEnd = visibleStart + visibleRows;
  const start = Math.max(0, Math.min(count, visibleStart - overscan));
  const end = Math.max(start, Math.min(count, visibleEnd + overscan));
  return {
    start,
    end,
    before: start * rowHeight,
    after: (count - end) * rowHeight,
  };
}

export function lineScrollTop(line: number): number {
  return Math.max(0, line - 1) * VIRTUAL_ROW_PX;
}
