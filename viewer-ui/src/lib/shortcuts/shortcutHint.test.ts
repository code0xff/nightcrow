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
    expect(shortcutKeys("project.next", null)).toEqual(["Ctrl+Shift+ArrowRight"]);
  });

  it("리더를_끄면_leader_액션에는_키가_없다", () => {
    // Advertising a missing key would document a no-op.
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
    // ARIA spaces mean alternatives, so `Control+F T` falsely claims that bare
    // `T` runs the action.
    expect(ariaKeyShortcuts("terminal.newPane", DEFAULT_LEADER)).toBeNull();
    expect(ariaKeyShortcuts("help.shortcuts", DEFAULT_LEADER)).toBeNull();
    // The human-readable title still spells out the sequence.
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
      // It must remain one chord; a space would turn it into alternatives.
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
