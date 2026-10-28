import { describe, expect, it } from "vitest";
import {
  applyRecovery,
  deadlineLabel,
  forgetRecovery,
  orphanRecovery,
  recoverySummary,
  type RecoveryByPane,
} from "./recovery";

/// A neutral placeholder: the core is provider-agnostic, and these strings come
/// from whatever plugin is installed.
const STATE = "waiting_for_reset";

describe("deadlineLabel", () => {
  it("formats epoch seconds as local time in HH:MM", () => {
    const epoch = 1_700_000_000;
    const expected = new Date(epoch * 1000);
    expect(deadlineLabel(epoch)).toBe(
      `${String(expected.getHours()).padStart(2, "0")}:${String(
        expected.getMinutes(),
      ).padStart(2, "0")}`,
    );
  });

  it("pads hours and minutes to two digits", () => {
    // Midnight UTC on the epoch: whatever the viewer's zone, both fields are
    // two characters wide.
    const label = deadlineLabel(0);
    expect(label).toMatch(/^\d{2}:\d{2}$/);
  });

  it("shows nothing when there is no deadline", () => {
    expect(deadlineLabel(undefined)).toBeUndefined();
  });

  it("shows nothing instead of an invalid time for values the clock cannot represent", () => {
    expect(deadlineLabel(Number.NaN)).toBeUndefined();
    expect(deadlineLabel(Number.POSITIVE_INFINITY)).toBeUndefined();
    // Past the range a `Date` can represent (±8.64e15 ms).
    expect(deadlineLabel(8.64e15)).toBeUndefined();
  });
});

describe("recoverySummary", () => {
  it("combines state, deadline, and attempt count on one line", () => {
    const summary = recoverySummary({
      state: STATE,
      deadlineEpoch: 1_700_000_000,
      attempt: 3,
    });
    expect(summary).toContain(STATE);
    expect(summary).toMatch(/until \d{2}:\d{2}/);
    expect(summary).toContain("attempt 3");
  });

  it("keeps only the state and no time when there is no deadline", () => {
    expect(recoverySummary({ state: STATE, attempt: 0 })).toBe(STATE);
  });

  it("omits the count when there are no attempts", () => {
    // Zero attempts is "not yet tried", which is not worth a number.
    expect(recoverySummary({ state: "resuming", attempt: 0 })).toBe("resuming");
  });
});

describe("applyRecovery", () => {
  it("records reports by pane", () => {
    const next = applyRecovery(
      {},
      {
        pane: 6,
        state: STATE,
        detail: "window closed",
        deadline_epoch: 1_700_000_000,
        attempt: 2,
      },
    );
    expect(next[6]).toEqual({
      state: STATE,
      detail: "window closed",
      deadlineEpoch: 1_700_000_000,
      attempt: 2,
    });
  });

  it("a new report replaces the previous report", () => {
    const first = applyRecovery({}, { pane: 6, state: STATE, attempt: 1 });
    const second = applyRecovery(first, {
      pane: 6,
      state: "backoff",
      attempt: 4,
    });
    expect(second[6]).toEqual({
      state: "backoff",
      detail: undefined,
      deadlineEpoch: undefined,
      attempt: 4,
    });
  });

  it("a cancelled report is not saved and removes the pane", () => {
    const held = applyRecovery({}, { pane: 6, state: STATE, attempt: 1 });
    const cleared = applyRecovery(held, {
      pane: 6,
      state: "cancelled",
      attempt: 0,
    });
    expect(cleared).toEqual({});
  });

  it("cancelled for a missing pane returns the same object", () => {
    // Object identity is what lets React skip a render for news-free frames.
    const held: RecoveryByPane = { 6: { state: STATE, attempt: 1 } };
    expect(applyRecovery(held, { pane: 9, state: "cancelled", attempt: 0 })).toBe(
      held,
    );
  });

  it("a report for another pane leaves existing panes untouched", () => {
    const held = applyRecovery({}, { pane: 6, state: STATE, attempt: 1 });
    const both = applyRecovery(held, { pane: 7, state: "backoff", attempt: 0 });
    expect(Object.keys(both).sort()).toEqual(["6", "7"]);
    expect(both[6]).toEqual(held[6]);
  });
});

describe("forgetRecovery", () => {
  it("removes the report when the pane closes", () => {
    const held = applyRecovery({}, { pane: 6, state: STATE, attempt: 1 });
    expect(forgetRecovery(held, 6)).toEqual({});
  });

  it("returns the same object for a pane with no report", () => {
    const held: RecoveryByPane = {};
    expect(forgetRecovery(held, 6)).toBe(held);
  });
});

describe("orphanRecovery", () => {
  it("selects only reports for panes not on the page", () => {
    const held = { 3: { state: STATE, attempt: 1 }, 6: { state: STATE, attempt: 1 } };
    expect(orphanRecovery(held, [6])).toEqual([3]);
  });

  it("sorts multiple reports by pane id", () => {
    const held = {
      9: { state: STATE, attempt: 0 },
      3: { state: STATE, attempt: 0 },
    };
    expect(orphanRecovery(held, [])).toEqual([3, 9]);
  });

  it("is empty when every report belongs to a live pane", () => {
    expect(orphanRecovery({ 6: { state: STATE, attempt: 0 } }, [6])).toEqual([]);
  });
});
