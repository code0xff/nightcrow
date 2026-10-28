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

describe("classifyShortcutKey ignored input", () => {
  it("ignores events that are not keydown", () => {
    for (const type of ["keypress", "keyup"]) {
      expect(classifyShortcutKey({ ...LEADER, type }, ctx())).toEqual({
        kind: "ignore",
      });
    }
  });

  it("ignores input during IME composition", () => {
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

  it("ignores repeated keydown while the key is held", () => {
    expect(classifyShortcutKey({ ...LEADER, repeat: true }, ctx())).toEqual({
      kind: "ignore",
    });
    expect(
      classifyShortcutKey(event({ key: "t", repeat: true }), ctx({ armed: true })),
    ).toEqual({ kind: "ignore" });
  });

  it("when suppressed, does not match a leader or action", () => {
    const suppressed = ctx({ suppressed: true });
    expect(classifyShortcutKey(LEADER, suppressed)).toEqual({ kind: "ignore" });
    expect(
      classifyShortcutKey(event({ key: "t" }), ctx({ armed: true, suppressed: true })),
    ).toEqual({ kind: "ignore" });
  });

  it("does not arm when the leader is inactive", () => {
    expect(classifyShortcutKey(LEADER, ctx({ leader: null }))).toEqual({
      kind: "ignore",
    });
  });

  it("ignores an unrelated key", () => {
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
  it("arms on the leader chord", () => {
    expect(classifyShortcutKey(LEADER, ctx())).toEqual({ kind: "arm" });
  });

  it("does not arm when extra modifiers are pressed with the leader", () => {
    expect(
      classifyShortcutKey(event({ key: "f", ctrlKey: true, shiftKey: true }), ctx()),
    ).toEqual({ kind: "ignore" });
  });

  it("a standalone chord is immediately an action", () => {
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

  it("does not match a standalone chord with extra modifiers", () => {
    expect(
      classifyShortcutKey(
        event({ key: "ArrowLeft", ctrlKey: true, shiftKey: true, metaKey: true }),
        ctx(),
      ),
    ).toEqual({ kind: "ignore" });
  });

  it("a standalone chord works without a leader", () => {
    const decision = classifyShortcutKey(
      event({ key: "ArrowLeft", ctrlKey: true, shiftKey: true }),
      ctx({ leader: null }),
    );
    expect(decision.kind).toBe("action");
  });
});

describe("classifyShortcutKey - armed", () => {
  const armed = ctx({ armed: true });

  it("a mapped follow-up is an action", () => {
    const decision = classifyShortcutKey(event({ key: "t" }), armed);
    expect(decision).toEqual({
      kind: "action",
      action: expect.objectContaining({ id: "terminal.newPane" }),
    });
  });

  it("ignores modifiers on a follow-up", () => {
    // The leader's Ctrl is usually still held when the follow-up arrives.
    expect(
      classifyShortcutKey(event({ key: "T", ctrlKey: true, shiftKey: true }), armed),
    ).toEqual({
      kind: "action",
      action: expect.objectContaining({ id: "terminal.newPane" }),
    });
  });

  it("Escape cancels", () => {
    expect(classifyShortcutKey(event({ key: "Escape" }), armed)).toEqual({
      kind: "cancel",
    });
  });

  it("Ctrl+C cancels", () => {
    expect(classifyShortcutKey(event({ key: "c", ctrlKey: true }), armed)).toEqual({
      kind: "cancel",
    });
  });

  it("Ctrl+Shift+C is the c command, not cancellation", () => {
    // Cancellation is exactly Ctrl+C; other modifiers are ignored on follow-up.
    expect(
      classifyShortcutKey(event({ key: "c", ctrlKey: true, shiftKey: true }), armed),
    ).toEqual({
      kind: "action",
      action: expect.objectContaining({ id: "terminal.cancelRecovery" }),
    });
  });

  it("pressing the leader twice sends a literal leader", () => {
    expect(classifyShortcutKey(LEADER, armed)).toEqual({
      kind: "literalLeader",
      data: "\x06",
    });
  });

  it("a leader without terminal encoding has no bytes to send", () => {
    const meta = parseChord("Meta+K")!;
    expect(
      classifyShortcutKey(event({ key: "k", metaKey: true }), ctx({ leader: meta, armed: true })),
    ).toEqual({ kind: "literalLeader", data: "" });
  });

  it("consumes an unmapped follow-up", () => {
    // docs/keybindings.md: "An unmapped follow-up is consumed."
    expect(classifyShortcutKey(event({ key: "j" }), armed)).toEqual({
      kind: "consumed",
    });
    expect(classifyShortcutKey(event({ key: "ArrowUp" }), armed)).toEqual({
      kind: "consumed",
    });
  });

  it("a modifier key by itself does not use the leader", () => {
    for (const key of ["Shift", "Control", "Alt", "Meta"]) {
      expect(classifyShortcutKey(event({ key }), armed)).toEqual({ kind: "ignore" });
    }
  });
});

describe("classifyShortcutKey Shift+Arrow focus cycling", () => {
  it("an arrow with only Shift pressed is a focus action", () => {
    expect(
      classifyShortcutKey(event({ key: "ArrowLeft", shiftKey: true }), ctx()),
    ).toEqual({ kind: "action", action: expect.objectContaining({ id: "focus.previous" }) });
    expect(
      classifyShortcutKey(event({ key: "ArrowRight", shiftKey: true }), ctx()),
    ).toEqual({ kind: "action", action: expect.objectContaining({ id: "focus.next" }) });
  });

  it("an arrow without Shift belongs to the pane", () => {
    // A bare arrow is a cursor key the program in the pane reads.
    expect(classifyShortcutKey(event({ key: "ArrowRight" }), ctx())).toEqual({
      kind: "ignore",
    });
  });

  it("with Ctrl added, it is project cycling rather than focus cycling", () => {
    // Exact modifiers, as `chordMatches` promises: the two chords share the key.
    const decision = classifyShortcutKey(
      event({ key: "ArrowRight", shiftKey: true, ctrlKey: true }),
      ctx(),
    );
    expect(decision).toEqual({ kind: "action", action: expect.objectContaining({ id: "project.next" }) });
  });
});
