import { describe, expect, it } from "vitest";
import {
  applyRecovery,
  cancelRecoveryFrame,
  deadlineLabel,
  forgetRecovery,
  orphanRecovery,
  recoverySummary,
  sendCancelRecovery,
  type RecoveryByPane,
} from "./recovery";

/// A neutral placeholder: the core is provider-agnostic, and these strings come
/// from whatever plugin is installed.
const STATE = "waiting_for_reset";

describe("deadlineLabel", () => {
  it("epoch초를_현지_시각_HH_MM으로_보여준다", () => {
    const epoch = 1_700_000_000;
    const expected = new Date(epoch * 1000);
    expect(deadlineLabel(epoch)).toBe(
      `${String(expected.getHours()).padStart(2, "0")}:${String(
        expected.getMinutes(),
      ).padStart(2, "0")}`,
    );
  });

  it("시와_분을_두_자리로_채운다", () => {
    // Midnight UTC on the epoch: whatever the viewer's zone, both fields are
    // two characters wide.
    const label = deadlineLabel(0);
    expect(label).toMatch(/^\d{2}:\d{2}$/);
  });

  it("deadline이_없으면_아무것도_보여주지_않는다", () => {
    expect(deadlineLabel(undefined)).toBeUndefined();
  });

  it("시계가_놓을_수_없는_값이면_틀린_시각_대신_아무것도_보여주지_않는다", () => {
    expect(deadlineLabel(Number.NaN)).toBeUndefined();
    expect(deadlineLabel(Number.POSITIVE_INFINITY)).toBeUndefined();
    // Past the range a `Date` can represent (±8.64e15 ms).
    expect(deadlineLabel(8.64e15)).toBeUndefined();
  });
});

describe("recoverySummary", () => {
  it("state와_deadline과_시도_횟수를_한_줄로_모은다", () => {
    const summary = recoverySummary({
      state: STATE,
      deadlineEpoch: 1_700_000_000,
      attempt: 3,
    });
    expect(summary).toContain(STATE);
    expect(summary).toMatch(/until \d{2}:\d{2}/);
    expect(summary).toContain("attempt 3");
  });

  it("deadline이_없으면_state만_남고_시각은_붙지_않는다", () => {
    expect(recoverySummary({ state: STATE, attempt: 0 })).toBe(STATE);
  });

  it("시도가_없으면_횟수를_붙이지_않는다", () => {
    // Zero attempts is "not yet tried", which is not worth a number.
    expect(recoverySummary({ state: "resuming", attempt: 0 })).toBe("resuming");
  });
});

describe("applyRecovery", () => {
  it("보고를_pane별로_기록한다", () => {
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

  it("새_보고가_이전_보고를_대체한다", () => {
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

  it("cancelled_보고는_저장하지_않고_pane을_지운다", () => {
    const held = applyRecovery({}, { pane: 6, state: STATE, attempt: 1 });
    const cleared = applyRecovery(held, {
      pane: 6,
      state: "cancelled",
      attempt: 0,
    });
    expect(cleared).toEqual({});
  });

  it("없는_pane의_cancelled는_같은_객체를_돌려준다", () => {
    // Object identity is what lets React skip a render for news-free frames.
    const held: RecoveryByPane = { 6: { state: STATE, attempt: 1 } };
    expect(applyRecovery(held, { pane: 9, state: "cancelled", attempt: 0 })).toBe(
      held,
    );
  });

  it("다른_pane의_보고는_기존_pane을_건드리지_않는다", () => {
    const held = applyRecovery({}, { pane: 6, state: STATE, attempt: 1 });
    const both = applyRecovery(held, { pane: 7, state: "backoff", attempt: 0 });
    expect(Object.keys(both).sort()).toEqual(["6", "7"]);
    expect(both[6]).toEqual(held[6]);
  });
});

describe("forgetRecovery", () => {
  it("pane이_닫히면_보고도_사라진다", () => {
    const held = applyRecovery({}, { pane: 6, state: STATE, attempt: 1 });
    expect(forgetRecovery(held, 6)).toEqual({});
  });

  it("보고가_없던_pane이면_같은_객체를_돌려준다", () => {
    const held: RecoveryByPane = {};
    expect(forgetRecovery(held, 6)).toBe(held);
  });
});

describe("sendCancelRecovery", () => {
  it("취소_컨트롤이_서버가_읽는_프레임을_보낸다", () => {
    const sent: string[] = [];
    sendCancelRecovery({ send: (data) => sent.push(data) }, 6);
    expect(sent).toEqual([`{"type":"cancel_recovery","pane":6}`]);
    expect(sent[0]).toBe(cancelRecoveryFrame(6));
  });

  it("소켓이_없으면_조용히_아무것도_하지_않는다", () => {
    expect(() => sendCancelRecovery(null, 6)).not.toThrow();
  });
});

describe("orphanRecovery", () => {
  it("페이지에_없는_pane의_보고만_고른다", () => {
    const held = { 3: { state: STATE, attempt: 1 }, 6: { state: STATE, attempt: 1 } };
    expect(orphanRecovery(held, [6])).toEqual([3]);
  });

  it("여러_개면_pane_id_순으로_정렬한다", () => {
    const held = {
      9: { state: STATE, attempt: 0 },
      3: { state: STATE, attempt: 0 },
    };
    expect(orphanRecovery(held, [])).toEqual([3, 9]);
  });

  it("모든_보고가_살아있는_pane의_것이면_비어_있다", () => {
    expect(orphanRecovery({ 6: { state: STATE, attempt: 0 } }, [6])).toEqual([]);
  });
});
