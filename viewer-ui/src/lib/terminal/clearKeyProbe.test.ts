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
  it("recognizes the clear key", () => {
    expect(isClearKey(keydown())).toBe(true);
  });

  it("is not a clear key when a modifier differs", () => {
    expect(isClearKey(keydown({ ctrlKey: false }))).toBe(false);
    expect(isClearKey(keydown({ metaKey: true }))).toBe(false);
    expect(isClearKey(keydown({ key: "k", code: "KeyK" }))).toBe(false);
    expect(isClearKey(keydown({ type: "keyup" }))).toBe(false);
  });
});

describe("ClearKeyProbe", () => {
  it("has nothing to report when there is no clear byte", () => {
    const probe = new ClearKeyProbe();
    probe.noteKey(keydown(), 0);
    expect(probe.report("ls -la\r", 1)).toBeNull();
  });

  it("attaches the preceding key event source to the byte", () => {
    const probe = new ClearKeyProbe();
    probe.noteKey(keydown({ isTrusted: false }), 100);

    expect(probe.report("\f", 104)).toEqual({
      key: { trusted: false, repeat: false, code: "KeyL", since_ms: 4 },
    });
  });

  it("distinguishes repeated key events while a key is held", () => {
    const probe = new ClearKeyProbe();
    probe.noteKey(keydown({ repeat: true }), 0);

    expect(probe.report("\f", 1)?.key?.repeat).toBe(true);
  });

  it("reports a byte received without a key event as such", () => {
    // A paste, an input method, or a script writing into the terminal.
    const probe = new ClearKeyProbe();

    expect(probe.report("\f", 0)).toEqual({ key: null });
  });

  it("a key event applies to only one byte", () => {
    // Two bytes from one keydown is itself a finding; reusing the event would
    // report the second as if a key had produced it.
    const probe = new ClearKeyProbe();
    probe.noteKey(keydown(), 0);

    expect(probe.report("\f", 1)?.key).not.toBeNull();
    expect(probe.report("\f", 2)).toEqual({ key: null });
  });

  it("an old key event is not treated as the byte source", () => {
    const probe = new ClearKeyProbe();
    probe.noteKey(keydown(), 0);

    expect(probe.report("\f", 5_000)).toEqual({ key: null });
  });
});
