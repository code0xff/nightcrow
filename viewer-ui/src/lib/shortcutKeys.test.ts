import { describe, expect, it } from "vitest";
import {
  classifyShortcutKey,
  type ShortcutContext,
  type ShortcutKeyEvent,
} from "./shortcutKeys";
import { DEFAULT_LEADER, parseChord } from "./leaderChord";

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

function ctx(over: Partial<ShortcutContext> = {}): ShortcutContext {
  return { leader: DEFAULT_LEADER, armed: false, suppressed: false, ...over };
}

const LEADER = event({ key: "f", ctrlKey: true });

describe("classifyShortcutKey - 무시하는 입력", () => {
  it("keydown이_아니면_무시한다", () => {
    for (const type of ["keypress", "keyup"]) {
      expect(classifyShortcutKey({ ...LEADER, type }, ctx())).toEqual({
        kind: "ignore",
      });
    }
  });

  it("IME_조합_중이면_무시한다", () => {
    const armed = ctx({ armed: true });
    expect(classifyShortcutKey(event({ key: "t", isComposing: true }), armed)).toEqual({
      kind: "ignore",
    });
    expect(classifyShortcutKey(event({ key: "t", keyCode: 229 }), armed)).toEqual({
      kind: "ignore",
    });
    expect(classifyShortcutKey(event({ key: "Process" }), armed)).toEqual({
      kind: "ignore",
    });
    expect(classifyShortcutKey(event({ key: "Unidentified" }), armed)).toEqual({
      kind: "ignore",
    });
  });

  it("키를_누른_채_반복되면_무시한다", () => {
    expect(classifyShortcutKey({ ...LEADER, repeat: true }, ctx())).toEqual({
      kind: "ignore",
    });
    expect(
      classifyShortcutKey(event({ key: "t", repeat: true }), ctx({ armed: true })),
    ).toEqual({ kind: "ignore" });
  });

  it("suppressed면_leader도_action도_잡지_않는다", () => {
    const suppressed = ctx({ suppressed: true });
    expect(classifyShortcutKey(LEADER, suppressed)).toEqual({ kind: "ignore" });
    expect(
      classifyShortcutKey(event({ key: "t" }), ctx({ armed: true, suppressed: true })),
    ).toEqual({ kind: "ignore" });
  });

  it("leader가_비활성이면_arm하지_않는다", () => {
    expect(classifyShortcutKey(LEADER, ctx({ leader: null }))).toEqual({
      kind: "ignore",
    });
  });

  it("아무_것도_아닌_키는_무시한다", () => {
    expect(classifyShortcutKey(event({ key: "j" }), ctx())).toEqual({ kind: "ignore" });
    expect(classifyShortcutKey(event({ key: "t", ctrlKey: true }), ctx())).toEqual({
      kind: "ignore",
    });
    expect(classifyShortcutKey(event({ key: "Escape" }), ctx())).toEqual({
      kind: "ignore",
    });
  });
});

describe("classifyShortcutKey - idle", () => {
  it("leader_chord는_arm한다", () => {
    expect(classifyShortcutKey(LEADER, ctx())).toEqual({ kind: "arm" });
  });

  it("수정자가_더_눌린_leader는_arm하지_않는다", () => {
    expect(
      classifyShortcutKey(event({ key: "f", ctrlKey: true, shiftKey: true }), ctx()),
    ).toEqual({ kind: "ignore" });
  });

  it("단독_chord는_바로_action이다", () => {
    expect(
      classifyShortcutKey(
        event({ key: "ArrowLeft", ctrlKey: true, shiftKey: true }),
        ctx(),
      ),
    ).toEqual({ kind: "action", action: expect.objectContaining({ id: "project.previous" }) });
    expect(
      classifyShortcutKey(
        event({ key: "ArrowRight", ctrlKey: true, shiftKey: true }),
        ctx(),
      ),
    ).toEqual({ kind: "action", action: expect.objectContaining({ id: "project.next" }) });
  });

  it("단독_chord도_수정자가_더_눌리면_잡지_않는다", () => {
    expect(
      classifyShortcutKey(
        event({ key: "ArrowLeft", ctrlKey: true, shiftKey: true, metaKey: true }),
        ctx(),
      ),
    ).toEqual({ kind: "ignore" });
  });

  it("leader가_없어도_단독_chord는_동작한다", () => {
    const decision = classifyShortcutKey(
      event({ key: "ArrowLeft", ctrlKey: true, shiftKey: true }),
      ctx({ leader: null }),
    );
    expect(decision.kind).toBe("action");
  });
});

describe("classifyShortcutKey - armed", () => {
  const armed = ctx({ armed: true });

  it("매핑된_follow_up은_action이다", () => {
    const decision = classifyShortcutKey(event({ key: "t" }), armed);
    expect(decision).toEqual({
      kind: "action",
      action: expect.objectContaining({ id: "terminal.newPane" }),
    });
  });

  it("follow_up의_수정자는_무시한다", () => {
    // leader의 Ctrl을 아직 놓지 않은 채 다음 키를 누르는 것이 보통이다.
    expect(
      classifyShortcutKey(event({ key: "T", ctrlKey: true, shiftKey: true }), armed),
    ).toEqual({
      kind: "action",
      action: expect.objectContaining({ id: "terminal.newPane" }),
    });
  });

  it("Escape는_취소다", () => {
    expect(classifyShortcutKey(event({ key: "Escape" }), armed)).toEqual({
      kind: "cancel",
    });
  });

  it("Ctrl_C는_취소다", () => {
    expect(classifyShortcutKey(event({ key: "c", ctrlKey: true }), armed)).toEqual({
      kind: "cancel",
    });
  });

  it("Ctrl_Shift_C는_취소가_아니라_c_명령이다", () => {
    // 취소는 정확히 Ctrl만 눌린 Ctrl+C이고, 그 밖의 수정자는 follow-up에서 무시된다.
    expect(
      classifyShortcutKey(event({ key: "c", ctrlKey: true, shiftKey: true }), armed),
    ).toEqual({
      kind: "action",
      action: expect.objectContaining({ id: "terminal.cancelRecovery" }),
    });
  });

  it("leader를_두_번_누르면_literal_leader를_보낸다", () => {
    expect(classifyShortcutKey(LEADER, armed)).toEqual({
      kind: "literalLeader",
      data: "\x06",
    });
  });

  it("터미널_인코딩이_없는_leader는_보낼_바이트가_없다", () => {
    const meta = parseChord("Meta+K")!;
    expect(
      classifyShortcutKey(event({ key: "k", metaKey: true }), ctx({ leader: meta, armed: true })),
    ).toEqual({ kind: "literalLeader", data: "" });
  });

  it("매핑되지_않은_follow_up은_소비된다", () => {
    // docs/keybindings.md: "An unmapped follow-up is consumed."
    expect(classifyShortcutKey(event({ key: "j" }), armed)).toEqual({
      kind: "consumed",
    });
    expect(classifyShortcutKey(event({ key: "ArrowUp" }), armed)).toEqual({
      kind: "consumed",
    });
  });

  it("수정자_키_단독은_leader를_쓰지_않는다", () => {
    for (const key of ["Shift", "Control", "Alt", "Meta"]) {
      expect(classifyShortcutKey(event({ key }), armed)).toEqual({ kind: "ignore" });
    }
  });
});

describe("classifyShortcutKey - Shift 화살표 포커스 순환", () => {
  it("Shift만_눌린_화살표는_포커스_액션이다", () => {
    expect(
      classifyShortcutKey(event({ key: "ArrowLeft", shiftKey: true }), ctx()),
    ).toEqual({ kind: "action", action: expect.objectContaining({ id: "focus.previous" }) });
    expect(
      classifyShortcutKey(event({ key: "ArrowRight", shiftKey: true }), ctx()),
    ).toEqual({ kind: "action", action: expect.objectContaining({ id: "focus.next" }) });
  });

  it("Shift_없는_화살표는_pane의_것이다", () => {
    // A bare arrow is a cursor key the program in the pane reads.
    expect(classifyShortcutKey(event({ key: "ArrowRight" }), ctx())).toEqual({
      kind: "ignore",
    });
  });

  it("Ctrl이_더_눌리면_프로젝트_순환이고_포커스_순환이_아니다", () => {
    // Exact modifiers, as `chordMatches` promises: the two chords share the key.
    const decision = classifyShortcutKey(
      event({ key: "ArrowRight", shiftKey: true, ctrlKey: true }),
      ctx(),
    );
    expect(decision).toEqual({ kind: "action", action: expect.objectContaining({ id: "project.next" }) });
  });
});
