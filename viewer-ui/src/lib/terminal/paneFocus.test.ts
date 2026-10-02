import { describe, expect, it } from "vitest";
import {
  focusFillsEmptyPanel,
  focusIsTakeable,
  focusOnAttach,
  focusStep,
  type FocusHolder,
} from "./paneFocus";

function holder(over: Partial<FocusHolder> = {}): FocusHolder {
  return { tagName: "DIV", editable: false, insidePanel: false, ...over };
}

describe("focusOnAttach", () => {
  it("comes back to the pane this screen last had the keyboard on", () => {
    expect(focusOnAttach(null, [1, 2, 3], 0, 2)).toBe(2);
  });

  it("takes the first pane when nothing is remembered", () => {
    // What the panel is already showing: `shownTab` puts the first pane on
    // screen while it waits for a focus.
    expect(focusOnAttach(null, [4, 5, 6], 0, undefined)).toBe(4);
  });

  it("takes the first pane when the remembered one is gone", () => {
    // A pane closed while this screen was away, or a session that has since
    // started numbering afresh.
    expect(focusOnAttach(null, [4, 5, 6], 0, 9)).toBe(4);
  });

  it("waits for the rest of the replay before guessing", () => {
    // The panes arrive one at a time. Guessing from the ones here would hold
    // the keyboard on a pane nobody chose for the rest of the replay — and the
    // remembered pane may be among the ones still coming.
    expect(focusOnAttach(null, [4], 2, undefined)).toBeNull();
    expect(focusOnAttach(null, [4], 2, 6)).toBeNull();
  });

  it("leaves the keyboard where it is once a pane has it", () => {
    // The socket makes the remembered pane active as it arrives, which is what
    // this must not talk over.
    expect(focusOnAttach(2, [1, 2, 3], 0, 3)).toBeNull();
  });

  it("focuses nothing when there are no panes", () => {
    expect(focusOnAttach(null, [], 0, undefined)).toBeNull();
    expect(focusOnAttach(null, [], 0, 2)).toBeNull();
  });
});

describe("focusStep", () => {
  it("focuses the active pane when the panel holds nothing", () => {
    expect(focusStep(1, true, null)).toEqual({ focus: true, held: 1 });
  });

  it("moves the keyboard when another pane becomes active", () => {
    expect(focusStep(2, true, 1)).toEqual({ focus: true, held: 2 });
  });

  it("leaves the keyboard alone on a layout change that moved no pane", () => {
    // The signals include every resize. A panel that already has the keyboard
    // must not take it again, or a divider drag would pull it back from
    // wherever the person put it.
    expect(focusStep(1, true, 1)).toEqual({ focus: false, held: 1 });
  });

  it("holds nothing while the active pane has no box to hold it", () => {
    // Hiding the panel blurs its terminal, and an xterm is not opened into a
    // cell with no size at all — so in both cases the panel holds nothing.
    expect(focusStep(1, false, 1)).toEqual({ focus: false, held: null });
  });

  it("focuses again when the pane gets a box back, without becoming active", () => {
    // The bug this exists for: leaving the terminal view and returning, where
    // `active` is the same pane throughout.
    const hidden = focusStep(1, false, 1);
    expect(focusStep(1, true, hidden.held)).toEqual({ focus: true, held: 1 });
  });

  it("focuses nothing while no pane is active", () => {
    expect(focusStep(null, true, null)).toEqual({ focus: false, held: null });
  });

  it("keeps holding the pane when nothing is active but the box is there", () => {
    // `active` is cleared for a frame when a pane exits and when a reconnect
    // starts; the panel has not lost the keyboard in between.
    expect(focusStep(null, true, 1)).toEqual({ focus: false, held: 1 });
  });
});

describe("focusIsTakeable", () => {
  it("takes the keyboard when nothing holds it", () => {
    expect(focusIsTakeable(null)).toBe(true);
  });

  it("takes it from the body, which is where hiding the panel leaves it", () => {
    // The case this exists to repair: below `md` the panel is `display: none`
    // while another view is chosen, and that blurs its terminal to the body.
    expect(focusIsTakeable(holder({ tagName: "BODY" }))).toBe(true);
  });

  it("takes it from the button that revealed the panel", () => {
    // The mobile view tabs are buttons, and the tap that brings the terminal
    // back is what leaves one holding the focus.
    expect(focusIsTakeable(holder({ tagName: "BUTTON" }))).toBe(true);
  });

  it("leaves a text field outside the panel alone", () => {
    // A resize re-asserts focus, so dragging the divider while the file filter
    // is being typed into must not pull the caret into a terminal.
    expect(focusIsTakeable(holder({ tagName: "INPUT" }))).toBe(false);
    expect(focusIsTakeable(holder({ tagName: "TEXTAREA" }))).toBe(false);
  });

  it("leaves an editable element outside the panel alone", () => {
    expect(focusIsTakeable(holder({ editable: true }))).toBe(false);
  });

  it("takes it from the panel's own textarea, which is another pane's", () => {
    // xterm keeps its caret in a hidden textarea. Refusing here would be
    // refusing to move the keyboard between panes at all.
    expect(
      focusIsTakeable(holder({ tagName: "TEXTAREA", insidePanel: true })),
    ).toBe(true);
  });

  it("takes it from an editable element inside the panel", () => {
    expect(focusIsTakeable(holder({ editable: true, insidePanel: true }))).toBe(
      true,
    );
  });
});

describe("focusFillsEmptyPanel", () => {
  it("hands the keyboard to `+` when the last pane leaves it nowhere", () => {
    expect(focusFillsEmptyPanel(0, 1, true)).toBe(true);
  });

  it("leaves focus that is somewhere else alone", () => {
    // The pane can die while the person is typing in the file filter. Their
    // caret is not what the panel emptying took away.
    expect(focusFillsEmptyPanel(0, 1, false)).toBe(false);
  });

  it("does nothing for a panel that was already empty", () => {
    // Every render of an empty panel would otherwise pull the keyboard back to
    // `+` — including the one where someone has just tabbed off it.
    expect(focusFillsEmptyPanel(0, 0, true)).toBe(false);
  });

  it("does nothing while panes remain", () => {
    // Closing one of several leaves the panel with a pane to focus, which is
    // `focusOnAttach`'s answer rather than this one.
    expect(focusFillsEmptyPanel(2, 3, true)).toBe(false);
    expect(focusFillsEmptyPanel(1, 0, true)).toBe(false);
  });
});
