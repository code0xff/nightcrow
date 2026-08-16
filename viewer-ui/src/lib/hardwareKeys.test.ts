import { describe, expect, it } from "vitest";
import { overriddenKeySequence, type TypedKey } from "./hardwareKeys";

function key(over: Partial<TypedKey> = {}): TypedKey {
  return {
    type: "keydown",
    key: "Enter",
    ctrlKey: false,
    altKey: false,
    metaKey: false,
    ...over,
  };
}

describe("overriddenKeySequence", () => {
  it("Ctrl_Enter는_LF를_보낸다", () => {
    // xterm은 CR을 보내고, TUI는 그것을 제출로 읽는다.
    expect(overriddenKeySequence(key({ ctrlKey: true }))).toBe("\n");
  });

  it("Ctrl_Alt_Enter는_ESC를_앞에_붙인다", () => {
    expect(overriddenKeySequence(key({ ctrlKey: true, altKey: true }))).toBe(
      "\x1b\n",
    );
  });

  it("맨_Enter는_xterm에게_맡긴다", () => {
    expect(overriddenKeySequence(key())).toBeNull();
    expect(overriddenKeySequence(key({ altKey: true }))).toBeNull();
  });

  it("Meta가_눌린_Enter는_xterm에게_맡긴다", () => {
    expect(
      overriddenKeySequence(key({ ctrlKey: true, metaKey: true })),
    ).toBeNull();
  });

  it("Enter가_아닌_키는_xterm에게_맡긴다", () => {
    expect(overriddenKeySequence(key({ key: "c", ctrlKey: true }))).toBeNull();
  });

  it("같은_타건의_keypress는_한_번_더_보내지_않는다", () => {
    expect(
      overriddenKeySequence(key({ type: "keypress", ctrlKey: true })),
    ).toBeNull();
  });
});
