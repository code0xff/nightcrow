import { describe, expect, it } from "vitest";
import {
  SHORTCUT_ACTIONS,
  UNSUPPORTED_TUI_ACTIONS,
  actionByLeader,
  actionById,
  focusPaneNumber,
  type ShortcutActionId,
} from "./shortcutActions";
import { formatChord, parseChord } from "./leaderChord";

describe("SHORTCUT_ACTIONS", () => {
  it("모든_액션_id는_유일하다", () => {
    const ids = SHORTCUT_ACTIONS.map((action) => action.id);
    expect(new Set(ids).size).toBe(ids.length);
  });

  it("leader_키는_한_액션에만_쓰인다", () => {
    const leaders = SHORTCUT_ACTIONS.flatMap((a) => (a.leader ? [a.leader] : []));
    expect(new Set(leaders).size).toBe(leaders.length);
  });

  it("leader_키는_소문자_한_글자나_숫자나_물음표다", () => {
    for (const action of SHORTCUT_ACTIONS) {
      if (!action.leader) continue;
      expect(action.leader).toMatch(/^[a-z0-9?]$/);
    }
  });

  it("모든_액션은_leader나_chord_중_하나만_가진다", () => {
    for (const action of SHORTCUT_ACTIONS) {
      const bound = [action.leader, action.chord].filter(Boolean);
      expect(bound).toHaveLength(1);
    }
  });

  it("chord는_정규_표시형과_왕복한다", () => {
    const chords = SHORTCUT_ACTIONS.flatMap((a) => (a.chord ? [a.chord] : []));
    expect(chords).toEqual(["Ctrl+Shift+ArrowLeft", "Ctrl+Shift+ArrowRight"]);
    for (const chord of chords) {
      const spec = parseChord(chord);
      expect(spec).not.toBeNull();
      expect(formatChord(spec!)).toBe(chord);
    }
  });

  it("fullscreen만_reinterpreted다", () => {
    // 브라우저에는 TUI의 fullscreen에 대응하는 것이 없어 의미만 옮긴다.
    const reinterpreted = SHORTCUT_ACTIONS.filter(
      (a) => a.support === "reinterpreted",
    ).map((a) => a.id);
    expect(reinterpreted).toEqual(["view.toggleMaximize"]);
    expect(actionById("view.toggleMaximize").note).toContain("F11");
  });

  it("TUI_문서의_leader_문자를_그대로_따른다", () => {
    // docs/keybindings.md가 두 구현의 기준이다.
    const expected: Record<string, ShortcutActionId> = {
      t: "terminal.newPane",
      w: "terminal.closePane",
      s: "terminal.swapPanePrompt",
      z: "terminal.claimSizing",
      c: "terminal.cancelRecovery",
      l: "view.toggleLog",
      b: "view.toggleTree",
      f: "view.toggleMaximize",
      o: "project.openDialog",
      x: "project.close",
      p: "session.cycleAccent",
      u: "session.reloadConfig",
      "1": "focus.list",
      "2": "focus.content",
      "?": "help.shortcuts",
    };
    for (const [key, id] of Object.entries(expected)) {
      expect(actionByLeader(key)?.id).toBe(id);
    }
  });

  it("숫자_3부터_9와_0은_pane_1부터_8을_가리킨다", () => {
    const digits = ["3", "4", "5", "6", "7", "8", "9", "0"];
    digits.forEach((digit, index) => {
      const action = actionByLeader(digit);
      expect(action?.id).toBe(`focus.pane${index + 1}`);
      expect(focusPaneNumber(action!.id)).toBe(index + 1);
    });
  });
});

describe("actionByLeader", () => {
  it("대문자_follow_up도_같은_액션을_찾는다", () => {
    expect(actionByLeader("T")?.id).toBe("terminal.newPane");
  });

  it("매핑되지_않은_키는_null이다", () => {
    expect(actionByLeader("j")).toBeNull();
    expect(actionByLeader("")).toBeNull();
    expect(actionByLeader("ArrowLeft")).toBeNull();
  });

  it("web에서_버린_TUI_키는_매핑되지_않는다", () => {
    for (const unsupported of UNSUPPORTED_TUI_ACTIONS) {
      if (unsupported.leader.length !== 1) continue;
      expect(actionByLeader(unsupported.leader)).toBeNull();
    }
  });
});

describe("UNSUPPORTED_TUI_ACTIONS", () => {
  it("redraw_detach_F키를_이유와_함께_기록한다", () => {
    const leaders = UNSUPPORTED_TUI_ACTIONS.map((a) => a.leader);
    expect(leaders).toEqual(["r", "q", "F1-F10"]);
    for (const action of UNSUPPORTED_TUI_ACTIONS) {
      expect(action.reason.length).toBeGreaterThan(0);
    }
  });
});

describe("actionById", () => {
  it("등록된_id는_같은_객체를_돌려준다", () => {
    expect(actionById("terminal.newPane").leader).toBe("t");
  });

  it("표에_없는_id는_던진다", () => {
    // 타입 union과 표가 어긋난 것이므로 조용한 기본값보다 실패가 낫다.
    expect(() => actionById("nope" as ShortcutActionId)).toThrow(/nope/);
  });
});

describe("focusPaneNumber", () => {
  it("pane_액션이_아니면_null이다", () => {
    expect(focusPaneNumber("focus.list")).toBeNull();
    expect(focusPaneNumber("terminal.newPane")).toBeNull();
  });
});
