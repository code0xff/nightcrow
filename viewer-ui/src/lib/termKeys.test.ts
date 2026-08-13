import { describe, expect, it } from "vitest";
import {
  KEYBOARD_MIN_VIEWPORT_PX,
  TERM_KEY_BAR,
  TERM_KEY_SEQUENCES,
  ctrlLatchStep,
  ctrlSequence,
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
    expect(termKeySequence("ctrl-b")).toBe("\x02");
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

describe("ctrlSequence", () => {
  it("글자는_대소문자와_무관하게_같은_제어바이트가_된다", () => {
    expect(ctrlSequence("a")).toBe("\x01");
    expect(ctrlSequence("A")).toBe("\x01");
    expect(ctrlSequence("e")).toBe("\x05");
    expect(ctrlSequence("k")).toBe("\x0b");
    expect(ctrlSequence("z")).toBe("\x1a");
  });

  it("괄호류와_스페이스_물음표도_터미널이_아는_제어바이트를_낸다", () => {
    expect(ctrlSequence("[")).toBe("\x1b");
    expect(ctrlSequence("\\")).toBe("\x1c");
    expect(ctrlSequence("]")).toBe("\x1d");
    expect(ctrlSequence("@")).toBe("\0");
    expect(ctrlSequence(" ")).toBe("\0");
    expect(ctrlSequence("?")).toBe("\x7f");
  });

  it("제어형이_없는_입력은_null이라_친_대로_흘러간다", () => {
    // Ctrl이 붙을 자리가 없는 것들. 한글은 IME가 내는 글자, 서로게이트 쌍은
    // 길이 2, 붙여넣기는 그보다 길다.
    expect(ctrlSequence("가")).toBeNull();
    expect(ctrlSequence("😀")).toBeNull();
    expect(ctrlSequence("ls -al")).toBeNull();
    expect(ctrlSequence("")).toBeNull();
    expect(ctrlSequence("1")).toBeNull();
    // 하드웨어 키보드가 이미 보낸 제어바이트에 두 번 붙지 않는다.
    expect(ctrlSequence("\x03")).toBeNull();
  });

  it("대문자가_두_글자가_되는_letter도_제어바이트가_아니다", () => {
    // 'ß'.toUpperCase()는 "SS"라 첫 글자만 보면 Ctrl-S가 되어버린다.
    expect(ctrlSequence("ß")).toBeNull();
  });
});

describe("ctrlLatchStep", () => {
  it("무장하지_않았으면_친_그대로_지나간다", () => {
    expect(ctrlLatchStep(false, "a")).toEqual({ data: "a", armed: false });
    expect(ctrlLatchStep(false, "\x1b[I")).toEqual({
      data: "\x1b[I",
      armed: false,
    });
  });

  it("무장한_뒤_친_글자는_제어바이트가_되고_래치가_풀린다", () => {
    expect(ctrlLatchStep(true, "a")).toEqual({ data: "\x01", armed: false });
    expect(ctrlLatchStep(true, "W")).toEqual({ data: "\x17", armed: false });
  });

  it("제어형이_없는_입력도_래치를_쓴다", () => {
    // 사람이 친 것은 맞으니 래치는 소진된다. 다만 글자는 그대로 나간다.
    expect(ctrlLatchStep(true, "가")).toEqual({ data: "가", armed: false });
    expect(ctrlLatchStep(true, "ls -al")).toEqual({
      data: "ls -al",
      armed: false,
    });
  });

  it("escape로_시작하는_입력은_래치를_쓰지_않는다", () => {
    // pane의 프로그램이 보내는 것들 — tmux·vim이 켜는 focus 리포트, 마우스
    // 리포트. 무장 직후 pane에 포커스를 주는 순간 제일 먼저 도착하므로, 이걸로
    // 소진되면 래치는 사람이 한 글자도 치기 전에 죽는다.
    expect(ctrlLatchStep(true, "\x1b[I")).toEqual({ data: "\x1b[I", armed: true });
    expect(ctrlLatchStep(true, "\x1b[<0;12;5M")).toEqual({
      data: "\x1b[<0;12;5M",
      armed: true,
    });
    // 하드웨어 키보드의 화살표·Esc도 자기 바이트를 이미 갖고 있다.
    expect(ctrlLatchStep(true, "\x1b[A")).toEqual({ data: "\x1b[A", armed: true });
    expect(ctrlLatchStep(true, "\x1b")).toEqual({ data: "\x1b", armed: true });
  });

  it("bracketed_paste도_래치를_남긴다", () => {
    // prefix 전체로 잡은 대가. 버튼이 켜진 채라 보이고 한 번 눌러 끌 수 있어,
    // 소리 없이 죽는 쪽보다 낫다는 판단이다.
    expect(ctrlLatchStep(true, "\x1b[200~ls\x1b[201~")).toEqual({
      data: "\x1b[200~ls\x1b[201~",
      armed: true,
    });
  });
});

describe("TERM_KEY_BAR", () => {
  it("모든_바_키_항목은_시퀀스_맵에_대응한다", () => {
    for (const item of TERM_KEY_BAR) {
      if (item.kind === "key") expect(TERM_KEY_SEQUENCES[item.key]).toBeDefined();
    }
  });

  it("Ctrl_래치는_바에_정확히_하나다", () => {
    // 눌린 상태를 가진 버튼이라 둘이면 어느 쪽이 켜졌는지 화면이 못 말한다.
    expect(TERM_KEY_BAR.filter((item) => item.kind === "ctrl")).toHaveLength(1);
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
