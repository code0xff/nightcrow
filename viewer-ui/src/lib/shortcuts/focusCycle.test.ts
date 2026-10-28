import { describe, expect, it } from "vitest";
import { nextFocus, type FocusRing } from "./focusCycle";

const list = { kind: "list" } as const;
const content = { kind: "content" } as const;
const pane = (index: number) => ({ kind: "pane", index }) as const;

function ring(over: Partial<FocusRing> = {}): FocusRing {
  return {
    at: list,
    paneCount: 2,
    maximized: "none",
    narrow: false,
    mobileView: "files",
    ...over,
  };
}

// The ring `src/app/navigation/focus.rs` walks: list, content, each pane, back to the list.
describe("nextFocus full-screen cycling", () => {
  it("forward from the list moves to content", () => {
    expect(nextFocus(ring({ at: list }), 1)).toEqual(content);
  });

  it("forward from content moves to the first pane", () => {
    expect(nextFocus(ring({ at: content }), 1)).toEqual(pane(0));
  });

  it("forward from a pane moves to the next pane", () => {
    expect(nextFocus(ring({ at: pane(0) }), 1)).toEqual(pane(1));
  });

  it("forward from the last pane returns to the list", () => {
    expect(nextFocus(ring({ at: pane(1) }), 1)).toEqual(list);
  });

  it("backward from the list moves to the last pane", () => {
    expect(nextFocus(ring({ at: list }), -1)).toEqual(pane(1));
  });

  it("backward from the first pane moves to content", () => {
    expect(nextFocus(ring({ at: pane(0) }), -1)).toEqual(content);
  });

  it("when there are no panes, content cycles to the list", () => {
    // `Focus::DiffViewer` with no panes goes straight back to the list, and the
    // list going back lands on the content pane rather than a pane that is not
    // there.
    expect(nextFocus(ring({ at: content, paneCount: 0 }), 1)).toEqual(list);
    expect(nextFocus(ring({ at: list, paneCount: 0 }), -1)).toEqual(content);
  });
});

describe("nextFocus when keyboard focus is outside the ring", () => {
  it("forward enters at the first position", () => {
    expect(nextFocus(ring({ at: null }), 1)).toEqual(list);
  });

  it("backward enters at the last position", () => {
    expect(nextFocus(ring({ at: null }), -1)).toEqual(pane(1));
  });

  it("keyboard focus on a removed pane also counts as outside the ring", () => {
    // `active` can name a pane that has just exited. Rather than walk from an
    // index the ring does not have, it re-enters at the near end.
    expect(nextFocus(ring({ at: pane(7) }), 1)).toEqual(list);
  });
});

describe("nextFocus when the terminal is maximized", () => {
  it("cycles panes and wraps at the end", () => {
    const maximized = ring({ maximized: "terminal", paneCount: 3 });
    expect(nextFocus({ ...maximized, at: pane(2) }, 1)).toEqual(pane(0));
    expect(nextFocus({ ...maximized, at: pane(0) }, -1)).toEqual(pane(2));
  });

  it("the list and content are not in the ring", () => {
    // The upper region is off screen, so the keyboard never lands on it.
    const maximized = ring({ maximized: "terminal", paneCount: 1 });
    expect(nextFocus({ ...maximized, at: null }, 1)).toEqual(pane(0));
    expect(nextFocus({ ...maximized, at: null }, -1)).toEqual(pane(0));
  });

  it("there is nowhere to move with one pane", () => {
    expect(
      nextFocus(ring({ maximized: "terminal", paneCount: 1, at: pane(0) }), 1),
    ).toBeNull();
  });

  it("there is nowhere to move with no panes", () => {
    expect(
      nextFocus(ring({ maximized: "terminal", paneCount: 0, at: null }), 1),
    ).toBeNull();
  });
});

describe("nextFocus when the upper area is maximized", () => {
  it("cycles only between the list and content", () => {
    // The panes are off screen, so the ring is the two spots that are not.
    const maximized = ring({ maximized: "files", paneCount: 3 });
    expect(nextFocus({ ...maximized, at: list }, 1)).toEqual(content);
    expect(nextFocus({ ...maximized, at: content }, 1)).toEqual(list);
    expect(nextFocus({ ...maximized, at: list }, -1)).toEqual(content);
  });

  it("keyboard focus on a pane counts as outside the ring", () => {
    expect(
      nextFocus(ring({ maximized: "files", paneCount: 3, at: pane(1) }), 1),
    ).toEqual(list);
  });
});

describe("nextFocus on narrow screens", () => {
  it("cycles panes in the terminal view", () => {
    const narrow = ring({ narrow: true, mobileView: "terminal", paneCount: 2 });
    expect(nextFocus({ ...narrow, at: pane(1) }, 1)).toEqual(pane(0));
    expect(nextFocus({ ...narrow, at: null }, -1)).toEqual(pane(1));
  });

  it("there is nowhere to move in the list view", () => {
    // The content pane and the panes are off screen; the one spot showing is
    // already where the keyboard is, or the only place for it to enter.
    const narrow = ring({ narrow: true, mobileView: "files", paneCount: 2 });
    expect(nextFocus({ ...narrow, at: list }, 1)).toBeNull();
    expect(nextFocus({ ...narrow, at: null }, 1)).toEqual(list);
  });

  it("does not move to a hidden pane from the content view", () => {
    // What this guards: an external keyboard on a phone must not change which
    // pane is active in a panel nobody can see.
    const narrow = ring({ narrow: true, mobileView: "diff", paneCount: 3 });
    expect(nextFocus({ ...narrow, at: content }, 1)).toBeNull();
    expect(nextFocus({ ...narrow, at: content }, -1)).toBeNull();
  });

  it("ignores maximized state on narrow screens", () => {
    // Below `md` the bottom navigation decides what is showing, not the panel
    // maximize — `RepoShell` hides the others regardless.
    const narrow = ring({
      narrow: true,
      mobileView: "files",
      maximized: "terminal",
      paneCount: 2,
    });
    expect(nextFocus({ ...narrow, at: null }, 1)).toEqual(list);
  });
});
