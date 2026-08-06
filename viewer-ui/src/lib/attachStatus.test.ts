import { describe, expect, it } from "vitest";
import { attachLabel, attachStatus } from "./attachStatus";

const state = {
  link: "live" as const,
  panes: 0,
  replayLeft: 0,
  pending: null,
};

describe("attachStatus", () => {
  it("waits for the socket before the session has said hello", () => {
    expect(attachStatus({ ...state, link: "connecting" })).toEqual({
      kind: "connecting",
    });
  });

  it("reports the link, not the panes, while the socket is gone", () => {
    // The panes of the socket that went are still on screen until the next
    // connect clears them, and nothing typed into them reaches a PTY.
    expect(attachStatus({ ...state, link: "reconnecting", panes: 2 })).toEqual({
      kind: "reconnecting",
    });
  });

  it("counts the panes a replay has promised and not yet delivered", () => {
    expect(attachStatus({ ...state, replayLeft: 3 })).toEqual({
      kind: "attaching",
      left: 3,
    });
  });

  it("keeps attaching while some of the replayed panes are here", () => {
    expect(attachStatus({ ...state, panes: 1, replayLeft: 2 })).toEqual({
      kind: "attaching",
      left: 2,
    });
  });

  it("is ready once the last replayed pane has arrived", () => {
    expect(attachStatus({ ...state, panes: 3 })).toEqual({ kind: "ready" });
  });

  it("names the startup terminals still being measured", () => {
    expect(attachStatus({ ...state, pending: 2 })).toEqual({
      kind: "starting",
      count: 2,
    });
  });

  it("is ready when panes exist alongside a startup still in flight", () => {
    // The panel renders startup slots only when it holds no panes, so this is
    // not a state it is waiting in.
    expect(attachStatus({ ...state, panes: 1, pending: 2 })).toEqual({
      kind: "ready",
    });
  });

  it("is empty only once the session is attached and holds nothing", () => {
    expect(attachStatus(state)).toEqual({ kind: "empty" });
  });
});

describe("attachLabel", () => {
  it("says nothing when the panel is attached", () => {
    expect(attachLabel({ kind: "ready" })).toBeNull();
    expect(attachLabel({ kind: "empty" })).toBeNull();
  });

  it("distinguishes a first attach from one the network took away", () => {
    expect(attachLabel({ kind: "connecting" })).toBe("Connecting…");
    expect(attachLabel({ kind: "reconnecting" })).toBe("Reconnecting…");
  });

  it("counts what is still coming", () => {
    expect(attachLabel({ kind: "attaching", left: 3 })).toBe(
      "Attaching 3 terminals…",
    );
    expect(attachLabel({ kind: "starting", count: 2 })).toBe(
      "Starting 2 terminals…",
    );
  });

  it("keeps the count singular for one terminal", () => {
    expect(attachLabel({ kind: "attaching", left: 1 })).toBe(
      "Attaching 1 terminal…",
    );
  });
});
