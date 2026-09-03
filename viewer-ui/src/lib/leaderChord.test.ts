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
  it("수정자와_키를_읽는다", () => {
    expect(parseChord("Ctrl+F")).toEqual(spec({ ctrl: true }));
    expect(parseChord("ctrl+shift+ArrowLeft")).toEqual(
      spec({ key: "ArrowLeft", ctrl: true, shift: true }),
    );
    expect(parseChord("Alt+G")).toEqual(spec({ key: "G", alt: true }));
    expect(parseChord("Meta+K")).toEqual(spec({ key: "K", meta: true }));
  });

  it("이름_있는_키의_대소문자를_정규화한다", () => {
    expect(parseChord("ctrl+pageup")?.key).toBe("PageUp");
    expect(parseChord("ctrl+escape")?.key).toBe("Escape");
    expect(parseChord("ctrl+f5")?.key).toBe("F5");
  });

  it("공백이_섞여도_읽는다", () => {
    expect(parseChord(" Ctrl + Shift + ArrowRight ")).toEqual(
      spec({ key: "ArrowRight", ctrl: true, shift: true }),
    );
  });

  it("빈_문자열은_null이다", () => {
    expect(parseChord("")).toBeNull();
    expect(parseChord("   ")).toBeNull();
  });

  it("수정자만_있으면_null이다", () => {
    expect(parseChord("Ctrl")).toBeNull();
    expect(parseChord("Ctrl+Shift")).toBeNull();
  });

  it("모르는_수정자는_null이다", () => {
    // Treating "Hyper" as a key would silently create an unreachable chord.
    expect(parseChord("Hyper+Ctrl+F")).toBeNull();
  });

  it("수정자가_중복되면_null이다", () => {
    expect(parseChord("Ctrl+Ctrl+F")).toBeNull();
  });

  it("키가_둘이면_null이다", () => {
    expect(parseChord("Ctrl+F+G")).toBeNull();
  });

  it("빈_조각이_있으면_null이다", () => {
    expect(parseChord("Ctrl+")).toBeNull();
    expect(parseChord("+F")).toBeNull();
    expect(parseChord("Ctrl++")).toBeNull();
  });
});

describe("formatChord", () => {
  it("정규_순서로_적는다", () => {
    expect(
      formatChord(spec({ key: "K", ctrl: true, alt: true, shift: true, meta: true })),
    ).toBe("Ctrl+Alt+Shift+Meta+K");
  });

  it("parseChord와_왕복한다", () => {
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
  it("같은_키와_수정자면_맞는다", () => {
    expect(chordMatches(spec({ ctrl: true }), event({ ctrlKey: true }))).toBe(true);
  });

  it("글자는_대소문자를_가리지_않는다", () => {
    expect(chordMatches(spec({ ctrl: true }), event({ key: "F", ctrlKey: true }))).toBe(
      true,
    );
  });

  it("수정자가_더_눌리면_맞지_않는다", () => {
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

  it("수정자가_모자라면_맞지_않는다", () => {
    expect(chordMatches(spec({ ctrl: true }), event())).toBe(false);
  });

  it("다른_키면_맞지_않는다", () => {
    expect(chordMatches(spec({ ctrl: true }), event({ key: "g", ctrlKey: true }))).toBe(
      false,
    );
  });

  it("Space는_공백_한_칸_키와_맞는다", () => {
    expect(chordMatches(spec({ key: "Space", ctrl: true }), event({ key: " ", ctrlKey: true }))).toBe(
      true,
    );
  });
});

describe("DEFAULT_LEADER", () => {
  it("TUI와_같은_Ctrl_F다", () => {
    expect(formatChord(DEFAULT_LEADER)).toBe("Ctrl+F");
  });
});

describe("leaderConflict", () => {
  it("브라우저가_이미_쓰는_Ctrl_조합을_경고한다", () => {
    for (const letter of ["F", "T", "N", "W", "L", "D", "P", "S", "R"]) {
      expect(leaderConflict(spec({ key: letter, ctrl: true }))).toBeTruthy();
      expect(leaderConflict(spec({ key: letter, meta: true }))).toBeTruthy();
    }
  });

  it("devtools_기본키를_경고한다", () => {
    for (const letter of ["I", "J", "C"]) {
      expect(
        leaderConflict(spec({ key: letter, ctrl: true, shift: true })),
      ).toBeTruthy();
    }
  });

  it("입력기_전환키를_경고한다", () => {
    expect(leaderConflict(spec({ key: "Space", meta: true }))).toBeTruthy();
    expect(leaderConflict(spec({ key: "Space", ctrl: true }))).toBeTruthy();
  });

  it("충돌을_모르는_chord는_null이다", () => {
    expect(leaderConflict(spec({ key: "G", alt: true }))).toBeNull();
    expect(leaderConflict(spec({ key: "B", ctrl: true }))).toBeNull();
    expect(leaderConflict(spec({ key: "ArrowLeft", ctrl: true, shift: true }))).toBeNull();
  });

  it("Shift가_붙으면_단일_수정자_충돌은_사라진다", () => {
    // Ctrl+Shift+F is not the browser's Find shortcut.
    expect(leaderConflict(spec({ key: "F", ctrl: true, shift: true }))).toBeNull();
  });

  it("경고에는_chord_표시형이_들어간다", () => {
    expect(leaderConflict(DEFAULT_LEADER)).toContain("Ctrl+F");
  });
});

describe("literalLeaderSequence", () => {
  it("Ctrl_글자는_C0_제어문자다", () => {
    // This matches `src/input/encode.rs` and `termKeys.ts::ctrlSequence`.
    expect(literalLeaderSequence(DEFAULT_LEADER)).toBe("\x06");
    expect(literalLeaderSequence(spec({ key: "A", ctrl: true }))).toBe("\x01");
    expect(literalLeaderSequence(spec({ key: "B", ctrl: true }))).toBe("\x02");
  });

  it("Ctrl_Alt는_ESC를_앞에_붙인다", () => {
    expect(literalLeaderSequence(spec({ key: "F", ctrl: true, alt: true }))).toBe(
      "\x1b\x06",
    );
  });

  it("Alt_키는_ESC와_그_문자다", () => {
    expect(literalLeaderSequence(spec({ key: "G", alt: true }))).toBe("\x1bg");
    expect(literalLeaderSequence(spec({ key: "G", alt: true, shift: true }))).toBe(
      "\x1bG",
    );
  });

  it("Ctrl_Space는_NUL이다", () => {
    expect(literalLeaderSequence(spec({ key: " ", ctrl: true }))).toBe("\0");
    expect(literalLeaderSequence(spec({ key: "Space", ctrl: true }))).toBe("\0");
  });

  it("수정자_없는_한_글자는_그_글자를_보낸다", () => {
    expect(literalLeaderSequence(spec({ key: "F" }))).toBe("f");
  });

  it("터미널_인코딩이_없는_chord는_null이다", () => {
    expect(literalLeaderSequence(spec({ key: "K", meta: true }))).toBeNull();
    expect(literalLeaderSequence(spec({ key: "ArrowLeft", ctrl: true }))).toBeNull();
    expect(literalLeaderSequence(spec({ key: "PageUp", alt: true }))).toBeNull();
  });
});
