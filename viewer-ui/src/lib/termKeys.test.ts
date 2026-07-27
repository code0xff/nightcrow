import { describe, expect, it } from "vitest";
import {
  TERM_KEY_BAR,
  TERM_KEY_SEQUENCES,
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
