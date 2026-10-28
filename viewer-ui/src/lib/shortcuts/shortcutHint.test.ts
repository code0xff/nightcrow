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
  it("leader actions have a leader code and follow-up key in two steps", () => {
    expect(shortcutKeys("terminal.newPane", DEFAULT_LEADER)).toEqual([
      "Ctrl+F",
      "t",
    ]);
  });

  it("chord actions use one code regardless of the leader", () => {
    expect(shortcutKeys("project.next", null)).toEqual(["Ctrl+Shift+ArrowRight"]);
  });

  it("leader actions have no key when the leader is disabled", () => {
    // Advertising a missing key would document a no-op.
    expect(shortcutKeys("terminal.newPane", null)).toBeNull();
    expect(shortcutHintText("terminal.newPane", null)).toBeNull();
    expect(ariaKeyShortcuts("terminal.newPane", null)).toBeNull();
  });

  it("changing the leader updates keys for every leader action", () => {
    const rebound = parseChord("Alt+Space");
    expect(shortcutKeys("view.toggleLog", rebound)).toEqual(["Alt+Space", "l"]);
  });
});

describe("ariaKeyShortcuts", () => {
  it("leader sequences have no aria attribute", () => {
    // ARIA spaces mean alternatives, so `Control+F T` falsely claims that bare
    // `T` runs the action.
    expect(ariaKeyShortcuts("terminal.newPane", DEFAULT_LEADER)).toBeNull();
    expect(ariaKeyShortcuts("help.shortcuts", DEFAULT_LEADER)).toBeNull();
    // The human-readable title still spells out the sequence.
    expect(shortcutHintText("terminal.newPane", DEFAULT_LEADER)).toBe(
      "Ctrl+F then t",
    );
  });

  it("only chord actions have the attribute, and Ctrl uses the W3C name Control", () => {
    expect(ariaKeyShortcuts("project.previous", DEFAULT_LEADER)).toBe(
      "Control+Shift+ArrowLeft",
    );
    expect(ariaKeyShortcuts("project.next", null)).toBe(
      "Control+Shift+ArrowRight",
    );
  });

  it("actions with a value exactly match actions bound to a chord", () => {
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
  it("the human-readable form joins two steps with then", () => {
    expect(shortcutHintText("session.reloadConfig", DEFAULT_LEADER)).toBe(
      "Ctrl+F then u",
    );
  });

  it("title is unchanged when there is no key", () => {
    expect(titleWithShortcut("Reload", null)).toBe("Reload");
    expect(titleWithShortcut("Reload", "Ctrl+F then u")).toBe(
      "Reload (Ctrl+F then u)",
    );
  });
});
