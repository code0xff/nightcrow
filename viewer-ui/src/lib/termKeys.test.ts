import { describe, expect, it } from "vitest";
import {
  KEYBOARD_MIN_VIEWPORT_PX,
  TERM_KEY_BAR,
  TERM_KEY_SEQUENCES,
  defaultKeyBarShown,
  parseKeyBarPref,
  termKeySequence,
  type TermKey,
} from "./termKeys";

describe("termKeySequence", () => {
  it("Esc는_ESC_바이트를_보낸다", () => {
    expect(termKeySequence("esc")).toBe("\x1b");
  });

  it("Tab은_수평탭을_보낸다", () => {
    expect(termKeySequence("tab")).toBe("\t");
  });

  it("Shift_Tab은_back_tab_CSI_Z를_보낸다", () => {
    expect(termKeySequence("shift-tab")).toBe("\x1b[Z");
    expect(termKeySequence("shift-tab", true)).toBe("\x1b[Z");
  });

  it("Ctrl_조합은_알파벳_순서의_제어바이트를_보낸다", () => {
    // Ctrl-C = 3rd letter = 0x03, and so on down the list.
    expect(termKeySequence("ctrl-c")).toBe("\x03");
    expect(termKeySequence("ctrl-d")).toBe("\x04");
    expect(termKeySequence("ctrl-z")).toBe("\x1a");
    expect(termKeySequence("ctrl-l")).toBe("\x0c");
    expect(termKeySequence("ctrl-r")).toBe("\x12");
  });

  it("일반_모드_화살표는_CSI_커서_시퀀스를_보낸다", () => {
    expect(termKeySequence("up")).toBe("\x1b[A");
    expect(termKeySequence("down")).toBe("\x1b[B");
    expect(termKeySequence("right")).toBe("\x1b[C");
    expect(termKeySequence("left")).toBe("\x1b[D");
  });

  it("application_커서_모드_화살표는_SS3_시퀀스를_보낸다", () => {
    // vim/less 등이 DECCKM을 켜면 ESC O A-D 로 바뀌어야 한다.
    expect(termKeySequence("up", true)).toBe("\x1bOA");
    expect(termKeySequence("down", true)).toBe("\x1bOB");
    expect(termKeySequence("right", true)).toBe("\x1bOC");
    expect(termKeySequence("left", true)).toBe("\x1bOD");
  });

  it("application_커서_모드는_화살표가_아닌_키에_영향을_주지_않는다", () => {
    expect(termKeySequence("esc", true)).toBe("\x1b");
    expect(termKeySequence("tab", true)).toBe("\t");
    expect(termKeySequence("ctrl-c", true)).toBe("\x03");
  });
});

describe("TERM_KEY_BAR", () => {
  it("모든_바_항목은_시퀀스_맵에_대응한다", () => {
    for (const { key } of TERM_KEY_BAR) {
      expect(TERM_KEY_SEQUENCES[key]).toBeDefined();
    }
  });

  it("바에_중복_키가_없다", () => {
    const keys = TERM_KEY_BAR.map((k) => k.label);
    expect(new Set(keys).size).toBe(keys.length);
  });

  it("모든_시퀀스는_비어있지_않다", () => {
    for (const key of Object.keys(TERM_KEY_SEQUENCES) as TermKey[]) {
      expect(termKeySequence(key).length).toBeGreaterThan(0);
    }
  });
});

describe("defaultKeyBarShown", () => {
  it("태블릿은_데스크톱만큼_넓어도_바를_받는다", () => {
    // 이 기능의 이유. 아이패드는 `md`보다 넓어서 폭으로만 판단하면 Esc도 ^C도
    // 없는 채로 남는다 — 정작 소프트키보드가 그 키들을 못 낸다.
    expect(defaultKeyBarShown(true, 1024)).toBe(true);
    expect(defaultKeyBarShown(true, 1366)).toBe(true);
  });

  it("마우스가_달린_넓은_화면은_바를_받지_않는다", () => {
    expect(defaultKeyBarShown(false, KEYBOARD_MIN_VIEWPORT_PX)).toBe(false);
    expect(defaultKeyBarShown(false, 1920)).toBe(false);
  });

  it("포인터를_모르는_좁은_화면은_폭으로_판단한다", () => {
    expect(defaultKeyBarShown(false, KEYBOARD_MIN_VIEWPORT_PX - 1)).toBe(true);
    expect(defaultKeyBarShown(false, 390)).toBe(true);
  });
});

describe("parseKeyBarPref", () => {
  it("저장한_값을_그대로_돌려준다", () => {
    expect(parseKeyBarPref("shown")).toBe("shown");
    expect(parseKeyBarPref("hidden")).toBe("hidden");
  });

  it("모르는_값은_거절해_기기_기본값이_살아난다", () => {
    expect(parseKeyBarPref(null)).toBeNull();
    expect(parseKeyBarPref("")).toBeNull();
    expect(parseKeyBarPref("true")).toBeNull();
    expect(parseKeyBarPref("Shown")).toBeNull();
  });
});
