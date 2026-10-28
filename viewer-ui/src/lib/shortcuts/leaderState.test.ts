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

describe("reduceLeader arm and disarm", () => {
  it("arming from idle arms the leader", () => {
    expect(reduceLeader(IDLE_LEADER, { kind: "arm" })).toEqual({
      state: ARMED,
      effect: { kind: "none" },
    });
  });

  it("cancel disarms the leader", () => {
    expect(reduceLeader(ARMED, { kind: "cancel" })).toEqual({
      state: IDLE_LEADER,
      effect: { kind: "none" },
    });
  });

  it("every external event disarms the leader", () => {
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

  it("ignore preserves the state", () => {
    expect(reduceLeader(ARMED, { kind: "ignore" })).toEqual({
      state: ARMED,
      effect: { kind: "none" },
    });
    expect(reduceLeader(IDLE_LEADER, { kind: "ignore" }).state).toEqual(IDLE_LEADER);
  });

  it("an unmapped follow-up disarms and does nothing", () => {
    expect(reduceLeader(ARMED, { kind: "consumed" })).toEqual({
      state: IDLE_LEADER,
      effect: { kind: "none" },
    });
  });

  it("literal_leader sends bytes and disarms", () => {
    expect(reduceLeader(ARMED, { kind: "literalLeader", data: "\x06" })).toEqual({
      state: IDLE_LEADER,
      effect: { kind: "sendLiteralLeader", data: "\x06" },
    });
  });
});

describe("reduceLeader - action", () => {
  it("a regular command runs and disarms", () => {
    expect(reduceLeader(ARMED, action("terminal.newPane"))).toEqual({
      state: IDLE_LEADER,
      effect: { kind: "run", action: "terminal.newPane" },
    });
  });

  it("a standalone chord runs immediately from idle", () => {
    expect(reduceLeader(IDLE_LEADER, action("project.next"))).toEqual({
      state: IDLE_LEADER,
      effect: { kind: "run", action: "project.next" },
    });
  });

  it("swap arms the second step instead of running an action", () => {
    expect(reduceLeader(ARMED, action("terminal.swapPanePrompt"))).toEqual({
      state: SWAP,
      effect: { kind: "none" },
    });
  });
});

describe("reduceLeader second step of swap", () => {
  it("a pane number emits swapWith and disarms", () => {
    for (let pane = 1; pane <= 8; pane += 1) {
      expect(reduceLeader(SWAP, action(`focus.pane${pane}` as ShortcutActionId))).toEqual({
        state: IDLE_LEADER,
        effect: { kind: "swapWith", pane },
      });
    }
  });

  it("Escape and Ctrl+C cancel the second step", () => {
    expect(reduceLeader(SWAP, { kind: "cancel" })).toEqual({
      state: IDLE_LEADER,
      effect: { kind: "none" },
    });
  });

  it("a non-pane key does nothing and disarms", () => {
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

describe("next key after disarming", () => {
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

  it("a terminal key after disarming is not swallowed", () => {
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

  it("a terminal key after completing a swap is not swallowed", () => {
    const { state } = reduceLeader(SWAP, action("focus.pane1"));
    const next = classifyShortcutKey(key("3"), {
      leader: DEFAULT_LEADER,
      armed: state.armed,
      suppressed: false,
    });
    expect(next).toEqual({ kind: "ignore" });
  });
});
