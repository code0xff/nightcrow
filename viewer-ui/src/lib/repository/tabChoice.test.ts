import { describe, expect, it, vi } from "vitest";
import { applyTabChoice, type TabChoiceOps } from "./tabChoice";

function ops(): TabChoiceOps & { calls: string[] } {
  const calls: string[] = [];
  return {
    calls,
    bumpPaneRequest: vi.fn(() => void calls.push("bumpPaneRequest")),
    leaveLog: vi.fn(() => void calls.push("leaveLog")),
    recordTab: vi.fn((next: string) => void calls.push(`recordTab:${next}`)),
    forgetPane: vi.fn(() => void calls.push("forgetPane")),
  };
}

describe("applyTabChoice", () => {
  it("does nothing when selecting the list already in view", () => {
    const spy = ops();

    expect(applyTabChoice("status", "status", spy)).toBe(false);

    expect(spy.calls).toEqual([]);
  });

  it("changing lists invalidates the request, records the choice, and clears the pane", () => {
    const spy = ops();

    expect(applyTabChoice("status", "tree", spy)).toBe(true);

    // The order matters: the request is invalidated before the new tab is
    // recorded, and the pane is emptied after.
    expect(spy.calls).toEqual([
      "bumpPaneRequest",
      "recordTab:tree",
      "forgetPane",
    ]);
  });

  it("clears the log only when leaving it", () => {
    const leaving = ops();
    applyTabChoice("log", "status", leaving);
    expect(leaving.calls).toEqual([
      "bumpPaneRequest",
      "leaveLog",
      "recordTab:status",
      "forgetPane",
    ]);

    const entering = ops();
    applyTabChoice("status", "log", entering);
    expect(entering.leaveLog).not.toHaveBeenCalled();

    const elsewhere = ops();
    applyTabChoice("tree", "status", elsewhere);
    expect(elsewhere.leaveLog).not.toHaveBeenCalled();
  });
});
