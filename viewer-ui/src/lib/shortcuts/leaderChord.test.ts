import { describe, expect, it } from "vitest";
import {
  DEFAULT_LEADER,
  chordMatches,
  formatChord,
  leaderConflict,
  literalLeaderSequence,
  parseChord,
  type ChordSpec,
} from "./leaderChord";
import type { ShortcutKeyEvent } from "./shortcutKeys";

function spec(over: Partial<ChordSpec> = {}): ChordSpec {
  return { key: "F", ctrl: false, shift: false, alt: false, meta: false, ...over };
}

function event(over: Partial<ShortcutKeyEvent> = {}): ShortcutKeyEvent {
  return {
    type: "keydown",
    key: "f",
    ctrlKey: false,
    shiftKey: false,
    altKey: false,
    metaKey: false,
    ...over,
  };
}

describe("parseChord", () => {
  it("parses modifiers and key", () => {
    expect(parseChord("Ctrl+F")).toEqual(spec({ ctrl: true }));
    expect(parseChord("ctrl+shift+ArrowLeft")).toEqual(
      spec({ key: "ArrowLeft", ctrl: true, shift: true }),
    );
    expect(parseChord("Alt+G")).toEqual(spec({ key: "G", alt: true }));
    expect(parseChord("Meta+K")).toEqual(spec({ key: "K", meta: true }));
  });

  it("normalizes the case of named keys", () => {
    expect(parseChord("ctrl+pageup")?.key).toBe("PageUp");
    expect(parseChord("ctrl+escape")?.key).toBe("Escape");
    expect(parseChord("ctrl+f5")?.key).toBe("F5");
  });

  it("parses input containing whitespace", () => {
    expect(parseChord(" Ctrl + Shift + ArrowRight ")).toEqual(
      spec({ key: "ArrowRight", ctrl: true, shift: true }),
    );
  });

  it("an empty string is null", () => {
    expect(parseChord("")).toBeNull();
    expect(parseChord("   ")).toBeNull();
  });

  it("modifiers without a key are null", () => {
    expect(parseChord("Ctrl")).toBeNull();
    expect(parseChord("Ctrl+Shift")).toBeNull();
  });

  it("an unknown modifier is null", () => {
    // Treating "Hyper" as a key would silently create an unreachable chord.
    expect(parseChord("Hyper+Ctrl+F")).toBeNull();
  });

  it("duplicate modifiers are null", () => {
    expect(parseChord("Ctrl+Ctrl+F")).toBeNull();
  });

  it("two keys are null", () => {
    expect(parseChord("Ctrl+F+G")).toBeNull();
  });

  it("an empty part is null", () => {
    expect(parseChord("Ctrl+")).toBeNull();
    expect(parseChord("+F")).toBeNull();
    expect(parseChord("Ctrl++")).toBeNull();
  });
});

describe("formatChord", () => {
  it("formats in canonical order", () => {
    expect(
      formatChord(spec({ key: "K", ctrl: true, alt: true, shift: true, meta: true })),
    ).toBe("Ctrl+Alt+Shift+Meta+K");
  });

  it("round-trips through parseChord", () => {
    for (const text of [
      "Ctrl+F",
      "Ctrl+Shift+ArrowLeft",
      "Alt+G",
      "Meta+K",
      "Ctrl+Alt+Shift+Meta+Space",
    ]) {
      expect(formatChord(parseChord(text)!)).toBe(text);
    }
  });
});

describe("chordMatches", () => {
  it("matches when the key and modifiers are the same", () => {
    expect(chordMatches(spec({ ctrl: true }), event({ ctrlKey: true }))).toBe(true);
  });

  it("letters are case-insensitive", () => {
    expect(chordMatches(spec({ ctrl: true }), event({ key: "F", ctrlKey: true }))).toBe(
      true,
    );
  });

  it("does not match when extra modifiers are pressed", () => {
    // Matching requires exact modifier equality, not containment.
    const left = spec({ key: "ArrowLeft", ctrl: true, shift: true });
    expect(
      chordMatches(left, event({ key: "ArrowLeft", ctrlKey: true, shiftKey: true })),
    ).toBe(true);
    expect(
      chordMatches(
        left,
        event({ key: "ArrowLeft", ctrlKey: true, shiftKey: true, metaKey: true }),
      ),
    ).toBe(false);
  });

  it("does not match when required modifiers are missing", () => {
    expect(chordMatches(spec({ ctrl: true }), event())).toBe(false);
  });

  it("does not match a different key", () => {
    expect(chordMatches(spec({ ctrl: true }), event({ key: "g", ctrlKey: true }))).toBe(
      false,
    );
  });

  it("Space matches a single-space key", () => {
    expect(chordMatches(spec({ key: "Space", ctrl: true }), event({ key: " ", ctrlKey: true }))).toBe(
      true,
    );
  });
});

describe("DEFAULT_LEADER", () => {
  it("uses the same Ctrl+F as the TUI", () => {
    expect(formatChord(DEFAULT_LEADER)).toBe("Ctrl+F");
  });
});

describe("leaderConflict", () => {
  it("warns about Ctrl combinations already used by the browser", () => {
    for (const letter of ["F", "T", "N", "W", "L", "D", "P", "S", "R"]) {
      expect(leaderConflict(spec({ key: letter, ctrl: true }))).toBeTruthy();
      expect(leaderConflict(spec({ key: letter, meta: true }))).toBeTruthy();
    }
  });

  it("warns about the default DevTools shortcut", () => {
    for (const letter of ["I", "J", "C"]) {
      expect(
        leaderConflict(spec({ key: letter, ctrl: true, shift: true })),
      ).toBeTruthy();
    }
  });

  it("warns about input-method switching keys", () => {
    expect(leaderConflict(spec({ key: "Space", meta: true }))).toBeTruthy();
    expect(leaderConflict(spec({ key: "Space", ctrl: true }))).toBeTruthy();
  });

  it("returns null when the chord conflict is unknown", () => {
    expect(leaderConflict(spec({ key: "G", alt: true }))).toBeNull();
    expect(leaderConflict(spec({ key: "B", ctrl: true }))).toBeNull();
    expect(leaderConflict(spec({ key: "ArrowLeft", ctrl: true, shift: true }))).toBeNull();
  });

  it("adding Shift removes a single-modifier conflict", () => {
    // Ctrl+Shift+F is not the browser's Find shortcut.
    expect(leaderConflict(spec({ key: "F", ctrl: true, shift: true }))).toBeNull();
  });

  it("the warning includes the chord display form", () => {
    expect(leaderConflict(DEFAULT_LEADER)).toContain("Ctrl+F");
  });
});

describe("literalLeaderSequence", () => {
  it("Ctrl plus a letter produces a C0 control character", () => {
    // This matches `src/input/encode.rs` and `lib/terminal/termKeys.ts::ctrlSequence`.
    expect(literalLeaderSequence(DEFAULT_LEADER)).toBe("\x06");
    expect(literalLeaderSequence(spec({ key: "A", ctrl: true }))).toBe("\x01");
    expect(literalLeaderSequence(spec({ key: "B", ctrl: true }))).toBe("\x02");
  });

  it("Ctrl+Alt prefixes ESC", () => {
    expect(literalLeaderSequence(spec({ key: "F", ctrl: true, alt: true }))).toBe(
      "\x1b\x06",
    );
  });

  it("Alt sends ESC followed by the character", () => {
    expect(literalLeaderSequence(spec({ key: "G", alt: true }))).toBe("\x1bg");
    expect(literalLeaderSequence(spec({ key: "G", alt: true, shift: true }))).toBe(
      "\x1bG",
    );
  });

  it("Ctrl+Space produces NUL", () => {
    expect(literalLeaderSequence(spec({ key: " ", ctrl: true }))).toBe("\0");
    expect(literalLeaderSequence(spec({ key: "Space", ctrl: true }))).toBe("\0");
  });

  it("a single character without modifiers sends that character", () => {
    expect(literalLeaderSequence(spec({ key: "F" }))).toBe("f");
  });

  it("a chord without terminal encoding is null", () => {
    expect(literalLeaderSequence(spec({ key: "K", meta: true }))).toBeNull();
    expect(literalLeaderSequence(spec({ key: "ArrowLeft", ctrl: true }))).toBeNull();
    expect(literalLeaderSequence(spec({ key: "PageUp", alt: true }))).toBeNull();
  });
});
