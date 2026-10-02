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
  it("이미_보고_있는_목록을_다시_고르면_아무_것도_하지_않는다", () => {
    const spy = ops();

    expect(applyTabChoice("status", "status", spy)).toBe(false);

    expect(spy.calls).toEqual([]);
  });

  it("목록을_바꾸면_요청을_무효화하고_기록하고_pane을_비운다", () => {
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

  it("로그를_떠날_때만_로그를_버린다", () => {
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
