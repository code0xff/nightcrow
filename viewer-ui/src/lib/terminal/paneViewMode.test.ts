import { describe, expect, it } from "vitest";
import {
  GRID_MIN_VIEWPORT_PX,
  canChoosePaneViewMode,
  defaultPaneViewMode,
  parsePaneViewMode,
  resolvePaneViewMode,
  shownTab,
  stackedCellStyle,
} from "./paneViewMode";

describe("defaultPaneViewMode", () => {
  it("tabs a screen too narrow for a split grid", () => {
    expect(defaultPaneViewMode(GRID_MIN_VIEWPORT_PX - 1)).toBe("tabs");
    expect(defaultPaneViewMode(390)).toBe("tabs");
  });

  it("keeps the grid from the breakpoint up", () => {
    expect(defaultPaneViewMode(GRID_MIN_VIEWPORT_PX)).toBe("grid");
    expect(defaultPaneViewMode(1440)).toBe("grid");
  });
});

describe("parsePaneViewMode", () => {
  it("takes back what it stored", () => {
    expect(parsePaneViewMode("grid")).toBe("grid");
    expect(parsePaneViewMode("tabs")).toBe("tabs");
  });

  it("rejects anything it does not recognise, so the default applies", () => {
    expect(parsePaneViewMode(null)).toBeNull();
    expect(parsePaneViewMode("")).toBeNull();
    expect(parsePaneViewMode("Grid")).toBeNull();
    expect(parsePaneViewMode("stack")).toBeNull();
  });
});

describe("shownTab", () => {
  it("shows the focused pane", () => {
    expect(shownTab(2, [1, 2, 3])).toBe(2);
  });

  it("falls back to the first pane while the focused one is not here", () => {
    // The frame between `exited` and the focus that follows it. There is no grid
    // behind the tabs, so showing nothing would blank the panel.
    expect(shownTab(2, [1, 3])).toBe(1);
  });

  it("falls back to the first pane when nothing is focused yet", () => {
    expect(shownTab(null, [4, 5])).toBe(4);
  });

  it("shows nothing when there are no panes at all", () => {
    expect(shownTab(null, [])).toBeNull();
    expect(shownTab(2, [])).toBeNull();
  });
});

describe("stackedCellStyle", () => {
  it("gives a hidden tab the same box as the shown one", () => {
    // A pane with no layout box measures zero, and zero is what defers opening
    // it and skips its resize — so switching tabs would cost a full repaint.
    const hidden = stackedCellStyle(false);
    expect(hidden.display).toBe("flex");
    expect(hidden.inset).toBe(0);
    expect(hidden.visibility).toBe("hidden");
  });

  it("shows the pane on top of the stack", () => {
    expect(stackedCellStyle(true).visibility).toBe("visible");
  });
});

describe("resolvePaneViewMode", () => {
  const narrow = GRID_MIN_VIEWPORT_PX - 1;
  const wide = GRID_MIN_VIEWPORT_PX;

  it("좁은_화면은_저장된_선택과_무관하게_탭이다", () => {
    // A grid there gives each pane too few columns, and a phone holding the
    // sizing in one would narrow every other screen's panes too.
    expect(resolvePaneViewMode("grid", narrow)).toBe("tabs");
    expect(resolvePaneViewMode(null, narrow)).toBe("tabs");
  });

  it("넓은_화면은_저장된_선택을_따르고_없으면_그리드다", () => {
    expect(resolvePaneViewMode("tabs", wide)).toBe("tabs");
    expect(resolvePaneViewMode("grid", wide)).toBe("grid");
    expect(resolvePaneViewMode(null, wide)).toBe("grid");
  });

  it("고를_수_있는지는_경계에서_갈린다", () => {
    expect(canChoosePaneViewMode(narrow)).toBe(false);
    expect(canChoosePaneViewMode(wide)).toBe(true);
  });
});
