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
  it("all action ids are unique", () => {
    const ids = SHORTCUT_ACTIONS.map((action) => action.id);
    expect(new Set(ids).size).toBe(ids.length);
  });

  it("each leader key is used by only one action", () => {
    const leaders = SHORTCUT_ACTIONS.flatMap((a) => (a.leader ? [a.leader] : []));
    expect(new Set(leaders).size).toBe(leaders.length);
  });

  it("a leader key is one lowercase letter, digit, or question mark", () => {
    for (const action of SHORTCUT_ACTIONS) {
      if (!action.leader) continue;
      expect(action.leader).toMatch(/^[a-z0-9?]$/);
    }
  });

  it("each action has either a leader or a chord", () => {
    for (const action of SHORTCUT_ACTIONS) {
      const bound = [action.leader, action.chord].filter(Boolean);
      expect(bound).toHaveLength(1);
    }
  });

  it("chords round-trip through canonical display form", () => {
    const chords = SHORTCUT_ACTIONS.flatMap((a) => (a.chord ? [a.chord] : []));
    expect(chords).toEqual([
      "Ctrl+Shift+ArrowLeft",
      "Ctrl+Shift+ArrowRight",
      "Shift+ArrowLeft",
      "Shift+ArrowRight",
    ]);
    for (const chord of chords) {
      const spec = parseChord(chord);
      expect(spec).not.toBeNull();
      expect(formatChord(spec!)).toBe(chord);
    }
  });

  it("keyboardOnly is set only on actions that arm a second step", () => {
    // Derived from the registry rather than listed by hand, so a new action
    // cannot be added with the flag — or without it — unnoticed. Arming a second
    // step is the only reason it may be set: see the field's doc comment.
    const keyboardOnly = SHORTCUT_ACTIONS.filter((a) => a.keyboardOnly).map(
      (a) => a.id,
    );
    expect(keyboardOnly).toEqual(["terminal.swapPanePrompt"]);
    for (const action of SHORTCUT_ACTIONS) {
      if (!action.keyboardOnly) continue;
      // The row is text, so its note is the only thing that can explain why
      // there is nothing to press, and a chord action has no second step.
      expect(action.note, action.id).toBeTruthy();
      expect(action.leader, action.id).toBeTruthy();
      expect(action.chord, action.id).toBeUndefined();
    }
  });

  it("only fullscreen is reinterpreted", () => {
    // The browser has no TUI fullscreen equivalent, so only the intent carries over.
    const reinterpreted = SHORTCUT_ACTIONS.filter(
      (a) => a.support === "reinterpreted",
    ).map((a) => a.id);
    expect(reinterpreted).toEqual(["view.toggleMaximize"]);
    expect(actionById("view.toggleMaximize").note).toContain("F11");
  });

  it("uses the leader characters from the TUI documentation unchanged", () => {
    // `docs/keybindings.md` is the source of truth for both implementations.
    const expected: Record<string, ShortcutActionId> = {
      t: "terminal.newPane",
      w: "terminal.closePane",
      s: "terminal.swapPanePrompt",
      r: "terminal.claimSizing",
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

  it("browser-only actions are limited to composing messages and do not use TUI leader keys", () => {
    const browserOnly = SHORTCUT_ACTIONS.filter((a) => a.support === "browserOnly");
    expect(browserOnly.map((a) => a.id)).toEqual(["terminal.composeMessage"]);
    // Keys the TUI binds (`docs/keybindings.md`), including the ones the web
    // leaves unbound: a browser-only command must not take any of them.
    const tuiLeaders = [..."twszclbfoxpurq[]"];
    for (const action of browserOnly) {
      expect(tuiLeaders, action.id).not.toContain(action.leader);
    }
  });

  it("TUI Shift+Arrow keys use the same keys for focus cycling", () => {
    // `src/input/routing.rs` binds Shift-only Left/Right to the focus ring; a
    // browser leaves Shift+Arrow to the page, so the web keeps the keys.
    const byChord = new Map(SHORTCUT_ACTIONS.map((a) => [a.chord, a.id]));
    expect(byChord.get("Shift+ArrowLeft")).toBe("focus.previous");
    expect(byChord.get("Shift+ArrowRight")).toBe("focus.next");
  });

  it("digits 3 through 9 and 0 target panes 1 through 8", () => {
    const digits = ["3", "4", "5", "6", "7", "8", "9", "0"];
    digits.forEach((digit, index) => {
      const action = actionByLeader(digit);
      expect(action?.id).toBe(`focus.pane${index + 1}`);
      expect(focusPaneNumber(action!.id)).toBe(index + 1);
    });
  });
});

describe("actionByLeader", () => {
  it("an uppercase follow-up finds the same action", () => {
    expect(actionByLeader("T")?.id).toBe("terminal.newPane");
  });

  it("an unmapped key is null", () => {
    expect(actionByLeader("j")).toBeNull();
    expect(actionByLeader("")).toBeNull();
    expect(actionByLeader("ArrowLeft")).toBeNull();
  });

  it("keeps the existing z binding as an escape hatch", () => {
    expect(actionByLeader("z")?.id).toBe("terminal.claimSizing");
  });

  it("TUI keys omitted on the web are not mapped", () => {
    for (const unsupported of UNSUPPORTED_TUI_ACTIONS) {
      if (unsupported.leader.length !== 1) continue;
      expect(actionByLeader(unsupported.leader)).toBeNull();
    }
  });
});

describe("UNSUPPORTED_TUI_ACTIONS", () => {
  it("records redraw, detach, and F keys with their reasons", () => {
    const leaders = UNSUPPORTED_TUI_ACTIONS.map((a) => a.leader);
    expect(leaders).toEqual(["q", "F1-F10"]);
    for (const action of UNSUPPORTED_TUI_ACTIONS) {
      expect(action.reason.length).toBeGreaterThan(0);
    }
  });
});

describe("actionById", () => {
  it("a registered id returns the same object", () => {
    expect(actionById("terminal.newPane").leader).toBe("t");
  });

  it("throws for an id that is not in the table", () => {
    // A union/table mismatch is a build error, not a case for a silent default.
    expect(() => actionById("nope" as ShortcutActionId)).toThrow(/nope/);
  });
});

describe("focusPaneNumber", () => {
  it("returns null for a non-pane action", () => {
    expect(focusPaneNumber("focus.list")).toBeNull();
    expect(focusPaneNumber("terminal.newPane")).toBeNull();
  });
});
