import { describe, expect, it } from "vitest";
import {
  browserHandlesKey,
  overriddenKeySequence,
  type TypedKey,
} from "./hardwareKeys";

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

describe("browserHandlesKey", () => {
  it("Ctrl_V는_브라우저에_맡긴다", () => {
    // 그러지 않으면 xterm이 \x16으로 인코딩해 paste 이벤트가 아예 없다.
    expect(browserHandlesKey(key({ key: "v", ctrlKey: true }))).toBe(true);
    expect(browserHandlesKey(key({ key: "V", ctrlKey: true }))).toBe(true);
  });

  it("Ctrl_없는_v는_그냥_타이핑이다", () => {
    expect(browserHandlesKey(key({ key: "v" }))).toBe(false);
  });

  it("Alt나_Meta가_섞인_조합은_pane의_것이다", () => {
    expect(
      browserHandlesKey(key({ key: "v", ctrlKey: true, altKey: true })),
    ).toBe(false);
    expect(
      browserHandlesKey(key({ key: "v", ctrlKey: true, metaKey: true })),
    ).toBe(false);
  });

  it("keydown이_아닌_이벤트는_답하지_않는다", () => {
    expect(
      browserHandlesKey(key({ type: "keypress", key: "v", ctrlKey: true })),
    ).toBe(false);
  });
});
