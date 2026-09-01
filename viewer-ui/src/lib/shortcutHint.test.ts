import { describe, expect, it } from "vitest";
import {
  ariaKeyShortcuts,
  shortcutHintText,
  shortcutKeys,
  titleWithShortcut,
} from "./shortcutHint";
import { DEFAULT_LEADER, parseChord } from "./leaderChord";
import { SHORTCUT_ACTIONS } from "./shortcutActions";

describe("shortcutKeys", () => {
  it("leader_액션은_리더_코드와_후속_키_두_단계다", () => {
    expect(shortcutKeys("terminal.newPane", DEFAULT_LEADER)).toEqual([
      "Ctrl+F",
      "t",
    ]);
  });

  it("chord_액션은_리더와_무관하게_그_코드_하나다", () => {
    // 리더를 꺼도 standalone chord는 살아 있다.
    expect(shortcutKeys("project.next", null)).toEqual(["Ctrl+Shift+ArrowRight"]);
  });

  it("리더를_끄면_leader_액션에는_키가_없다", () => {
    // 없는 키를 있다고 말하면 아무 일도 하지 않는 키를 안내하게 된다.
    expect(shortcutKeys("terminal.newPane", null)).toBeNull();
    expect(shortcutHintText("terminal.newPane", null)).toBeNull();
    expect(ariaKeyShortcuts("terminal.newPane", null)).toBeNull();
  });

  it("리더를_바꾸면_모든_leader_액션의_키가_따라온다", () => {
    const rebound = parseChord("Alt+Space");
    expect(shortcutKeys("view.toggleLog", rebound)).toEqual(["Alt+Space", "l"]);
  });
});

describe("ariaKeyShortcuts", () => {
  it("leader_시퀀스는_속성을_받지_않는다", () => {
    // ARIA의 공백은 "대안"이라서 `Control+F T`는 T 하나로도 실행된다는 주장이
    // 된다. 그런 키는 없으므로 틀린 안내가 되고, 없는 것보다 나쁘다.
    expect(ariaKeyShortcuts("terminal.newPane", DEFAULT_LEADER)).toBeNull();
    expect(ariaKeyShortcuts("help.shortcuts", DEFAULT_LEADER)).toBeNull();
    // 사람이 읽는 쪽은 그대로 시퀀스를 말한다.
    expect(shortcutHintText("terminal.newPane", DEFAULT_LEADER)).toBe(
      "Ctrl+F then t",
    );
  });

  it("chord_액션만_속성을_받고_Ctrl은_W3C_이름인_Control로_나간다", () => {
    expect(ariaKeyShortcuts("project.previous", DEFAULT_LEADER)).toBe(
      "Control+Shift+ArrowLeft",
    );
    expect(ariaKeyShortcuts("project.next", null)).toBe(
      "Control+Shift+ArrowRight",
    );
  });

  it("값이_있는_액션은_chord로_묶인_액션과_정확히_일치한다", () => {
    for (const action of SHORTCUT_ACTIONS) {
      const value = ariaKeyShortcuts(action.id, DEFAULT_LEADER);
      if (!action.chord) {
        expect(value, action.id).toBeNull();
        continue;
      }
      // 하나의 chord여야 한다: 공백이 들어가면 대안 목록으로 읽힌다.
      expect(value, action.id).toMatch(/^([A-Za-z]+\+)*[^+\s]+$/);
    }
  });
});

describe("shortcutHintText", () => {
  it("사람이_읽는_형태는_두_단계를_then으로_잇는다", () => {
    expect(shortcutHintText("session.reloadConfig", DEFAULT_LEADER)).toBe(
      "Ctrl+F then u",
    );
  });

  it("키가_없으면_title은_그대로다", () => {
    expect(titleWithShortcut("Reload", null)).toBe("Reload");
    expect(titleWithShortcut("Reload", "Ctrl+F then u")).toBe(
      "Reload (Ctrl+F then u)",
    );
  });
});
