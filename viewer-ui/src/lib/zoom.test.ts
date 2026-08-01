import { describe, expect, it } from "vitest";
import { renderedZoom, zoomPending, zoomRequest } from "./zoom";

describe("renderedZoom", () => {
  it("fills the panel with the pane the server named", () => {
    expect(renderedZoom(2, [1, 2, 3])).toBe(2);
  });

  it("falls back to the grid when nothing is zoomed", () => {
    expect(renderedZoom(null, [1, 2])).toBeNull();
  });

  it("falls back to the grid while the zoomed pane is not here", () => {
    // The frame between `exited` and the `zoomed` that ends it. Rendering the
    // raw value here hides every cell, because none of them is the named pane.
    expect(renderedZoom(2, [1, 3])).toBeNull();
  });

  it("falls back to the grid when there are no panes at all", () => {
    // A reconnect: the zoom can be replayed before the panes it names.
    expect(renderedZoom(2, [])).toBeNull();
  });
});

describe("zoomRequest", () => {
  it("asks for the pane when nothing is zoomed", () => {
    expect(zoomRequest(null, 3)).toBe(3);
  });

  it("asks for the grid when the pane is the one filling the panel", () => {
    expect(zoomRequest(3, 3)).toBeNull();
  });

  it("moves the zoom when another pane is filling the panel", () => {
    expect(zoomRequest(1, 3)).toBe(3);
  });

  it("un-zooms on a second press made before the first was answered", () => {
    // The zoom is applied on the server's echo, so a quick double press has
    // only the first request to judge the second against. Reading both against
    // the pre-press state would re-send the first, the server would find
    // nothing to change, and the pane would stay zoomed.
    const first = zoomRequest(null, 3);
    expect(first).toBe(3);
    expect(zoomRequest(first, 3)).toBeNull();
  });
});

describe("zoomPending", () => {
  it("waits while the pane a replayed zoom names has not arrived", () => {
    expect(zoomPending(2, [])).toBe(true);
    expect(zoomPending(2, [1])).toBe(true);
  });

  it("stops waiting once that pane is here", () => {
    expect(zoomPending(2, [1, 2])).toBe(false);
  });

  it("never waits when nothing is zoomed", () => {
    expect(zoomPending(null, [])).toBe(false);
    expect(zoomPending(null, [1, 2])).toBe(false);
  });
});
