import { describe, expect, it } from "vitest";
import { ClearKeyProbe, isClearKey } from "./clearKeyProbe";

/// A stand-in for the fields the probe reads off a real `KeyboardEvent`.
function keydown(overrides: Partial<KeyboardEvent> = {}): KeyboardEvent {
  return {
    type: "keydown",
    ctrlKey: true,
    altKey: false,
    metaKey: false,
    key: "l",
    code: "KeyL",
    isTrusted: true,
    repeat: false,
    ...overrides,
  } as KeyboardEvent;
}

describe("isClearKey", () => {
  it("클리어_키를_알아본다", () => {
    expect(isClearKey(keydown())).toBe(true);
  });

  it("수정자가_다르면_클리어_키가_아니다", () => {
    expect(isClearKey(keydown({ ctrlKey: false }))).toBe(false);
    expect(isClearKey(keydown({ metaKey: true }))).toBe(false);
    expect(isClearKey(keydown({ key: "k", code: "KeyK" }))).toBe(false);
    expect(isClearKey(keydown({ type: "keyup" }))).toBe(false);
  });
});

describe("ClearKeyProbe", () => {
  it("클리어_바이트가_없으면_보고할_것이_없다", () => {
    const probe = new ClearKeyProbe();
    probe.noteKey(keydown(), 0);
    expect(probe.report("ls -la\r", 1)).toBeNull();
  });

  it("직전_키_이벤트의_출처를_바이트에_붙인다", () => {
    const probe = new ClearKeyProbe();
    probe.noteKey(keydown({ isTrusted: false }), 100);

    expect(probe.report("\f", 104)).toEqual({
      key: { trusted: false, repeat: false, code: "KeyL", since_ms: 4 },
    });
  });

  it("눌린_채로_반복되는_키를_구분한다", () => {
    const probe = new ClearKeyProbe();
    probe.noteKey(keydown({ repeat: true }), 0);

    expect(probe.report("\f", 1)?.key?.repeat).toBe(true);
  });

  it("키_이벤트_없이_온_바이트는_그렇게_보고한다", () => {
    // A paste, an input method, or a script writing into the terminal.
    const probe = new ClearKeyProbe();

    expect(probe.report("\f", 0)).toEqual({ key: null });
  });

  it("한_키_이벤트는_한_바이트에만_쓰인다", () => {
    // Two bytes from one keydown is itself a finding; reusing the event would
    // report the second as if a key had produced it.
    const probe = new ClearKeyProbe();
    probe.noteKey(keydown(), 0);

    expect(probe.report("\f", 1)?.key).not.toBeNull();
    expect(probe.report("\f", 2)).toEqual({ key: null });
  });

  it("오래된_키_이벤트는_바이트의_출처로_치지_않는다", () => {
    const probe = new ClearKeyProbe();
    probe.noteKey(keydown(), 0);

    expect(probe.report("\f", 5_000)).toEqual({ key: null });
  });
});
