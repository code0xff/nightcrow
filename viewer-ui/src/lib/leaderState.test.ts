import { describe, expect, it } from "vitest";
import {
  IDLE_LEADER,
  reduceLeader,
  type LeaderEvent,
  type LeaderState,
} from "./leaderState";
import { actionById } from "./shortcutActions";
import type { ShortcutActionId } from "./shortcutActions";
import { classifyShortcutKey } from "./shortcutKeys";
import { DEFAULT_LEADER } from "./leaderChord";

const ARMED: LeaderState = { armed: true, swapPending: false };
const SWAP: LeaderState = { armed: true, swapPending: true };

function action(id: ShortcutActionId): LeaderEvent {
  return { kind: "action", action: actionById(id) };
}

describe("reduceLeader - arm과 disarm", () => {
  it("idle에서_arm하면_무장한다", () => {
    expect(reduceLeader(IDLE_LEADER, { kind: "arm" })).toEqual({
      state: ARMED,
      effect: { kind: "none" },
    });
  });

  it("cancel은_무장을_푼다", () => {
    expect(reduceLeader(ARMED, { kind: "cancel" })).toEqual({
      state: IDLE_LEADER,
      effect: { kind: "none" },
    });
  });

  it("바깥에서_생긴_모든_사건은_무장을_푼다", () => {
    // If it stays armed, the next terminal key disappears silently.
    const events: LeaderEvent["kind"][] = [
      "blur",
      "focusChange",
      "dialogOpen",
      "repoChange",
      "socketReconnect",
      "disabled",
      "suppressed",
    ];
    for (const kind of events) {
      expect(reduceLeader(ARMED, { kind } as LeaderEvent)).toEqual({
        state: IDLE_LEADER,
        effect: { kind: "none" },
      });
      expect(reduceLeader(SWAP, { kind } as LeaderEvent).state).toEqual(IDLE_LEADER);
    }
  });

  it("ignore는_상태를_그대로_둔다", () => {
    expect(reduceLeader(ARMED, { kind: "ignore" })).toEqual({
      state: ARMED,
      effect: { kind: "none" },
    });
    expect(reduceLeader(IDLE_LEADER, { kind: "ignore" }).state).toEqual(IDLE_LEADER);
  });

  it("매핑되지_않은_follow_up은_무장을_풀고_아무_것도_하지_않는다", () => {
    expect(reduceLeader(ARMED, { kind: "consumed" })).toEqual({
      state: IDLE_LEADER,
      effect: { kind: "none" },
    });
  });

  it("literal_leader는_바이트를_보내고_무장을_푼다", () => {
    expect(reduceLeader(ARMED, { kind: "literalLeader", data: "\x06" })).toEqual({
      state: IDLE_LEADER,
      effect: { kind: "sendLiteralLeader", data: "\x06" },
    });
  });
});

describe("reduceLeader - action", () => {
  it("보통_명령은_실행하고_무장을_푼다", () => {
    expect(reduceLeader(ARMED, action("terminal.newPane"))).toEqual({
      state: IDLE_LEADER,
      effect: { kind: "run", action: "terminal.newPane" },
    });
  });

  it("단독_chord는_idle에서_바로_실행한다", () => {
    expect(reduceLeader(IDLE_LEADER, action("project.next"))).toEqual({
      state: IDLE_LEADER,
      effect: { kind: "run", action: "project.next" },
    });
  });

  it("swap은_실행_대신_두_번째_단계를_무장한다", () => {
    expect(reduceLeader(ARMED, action("terminal.swapPanePrompt"))).toEqual({
      state: SWAP,
      effect: { kind: "none" },
    });
  });
});

describe("reduceLeader - swap의 두 번째 단계", () => {
  it("pane_숫자는_swapWith를_내고_무장을_푼다", () => {
    for (let pane = 1; pane <= 8; pane += 1) {
      expect(reduceLeader(SWAP, action(`focus.pane${pane}` as ShortcutActionId))).toEqual({
        state: IDLE_LEADER,
        effect: { kind: "swapWith", pane },
      });
    }
  });

  it("Escape와_Ctrl_C는_두_번째_단계를_취소한다", () => {
    expect(reduceLeader(SWAP, { kind: "cancel" })).toEqual({
      state: IDLE_LEADER,
      effect: { kind: "none" },
    });
  });

  it("pane_숫자가_아닌_키는_아무_것도_하지_않고_무장을_푼다", () => {
    // A swap request must not execute an unrelated command.
    for (const id of ["focus.list", "focus.content", "terminal.newPane"] as const) {
      expect(reduceLeader(SWAP, action(id))).toEqual({
        state: IDLE_LEADER,
        effect: { kind: "none" },
      });
    }
    expect(reduceLeader(SWAP, { kind: "consumed" }).effect).toEqual({ kind: "none" });
  });
});

describe("무장 해제 뒤의 다음 키", () => {
  function key(char: string) {
    return {
      type: "keydown",
      key: char,
      ctrlKey: false,
      shiftKey: false,
      altKey: false,
      metaKey: false,
    };
  }

  it("무장이_풀린_뒤의_터미널_키는_삼켜지지_않는다", () => {
    const disarmers: LeaderEvent[] = [
      { kind: "cancel" },
      { kind: "consumed" },
      { kind: "blur" },
      { kind: "focusChange" },
      { kind: "dialogOpen" },
      { kind: "repoChange" },
      { kind: "socketReconnect" },
      { kind: "disabled" },
      { kind: "suppressed" },
      action("terminal.newPane"),
    ];
    for (const disarmer of disarmers) {
      const { state } = reduceLeader(ARMED, disarmer);
      expect(state.armed).toBe(false);
      const next = classifyShortcutKey(key("t"), {
        leader: DEFAULT_LEADER,
        armed: state.armed,
        suppressed: false,
      });
      expect(next).toEqual({ kind: "ignore" });
    }
  });

  it("swap을_끝낸_뒤의_터미널_키도_삼켜지지_않는다", () => {
    const { state } = reduceLeader(SWAP, action("focus.pane1"));
    const next = classifyShortcutKey(key("3"), {
      leader: DEFAULT_LEADER,
      armed: state.armed,
      suppressed: false,
    });
    expect(next).toEqual({ kind: "ignore" });
  });
});
