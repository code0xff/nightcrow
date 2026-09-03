import { describe, expect, it } from "vitest";
import {
  isTextEntryTarget,
  shortcutsSuppressed,
  type TargetDescription,
} from "./shortcutTarget";

function target(over: Partial<TargetDescription> = {}): TargetDescription {
  return { tagName: "DIV", ...over };
}

describe("isTextEntryTarget", () => {
  it("타이핑을_받는_요소는_참이다", () => {
    expect(isTextEntryTarget(target({ tagName: "INPUT" }))).toBe(true);
    expect(isTextEntryTarget(target({ tagName: "TEXTAREA" }))).toBe(true);
    expect(isTextEntryTarget(target({ tagName: "SELECT" }))).toBe(true);
    expect(isTextEntryTarget(target({ isContentEditable: true }))).toBe(true);
  });

  it("텍스트_입력_역할을_가진_요소는_참이다", () => {
    for (const role of ["textbox", "searchbox", "combobox"]) {
      expect(isTextEntryTarget(target({ role }))).toBe(true);
    }
    expect(isTextEntryTarget(target({ role: "SearchBox" }))).toBe(true);
  });

  it("type이_있는_텍스트_input은_참이다", () => {
    for (const type of ["text", "password", "search", "email", "url", "number"]) {
      expect(isTextEntryTarget(target({ tagName: "INPUT", type }))).toBe(true);
    }
  });

  it("글자를_받지_않는_input은_거짓이다", () => {
    for (const type of [
      "checkbox",
      "radio",
      "button",
      "submit",
      "reset",
      "image",
      "range",
      "color",
      "file",
    ]) {
      expect(isTextEntryTarget(target({ tagName: "INPUT", type }))).toBe(false);
    }
    expect(isTextEntryTarget(target({ tagName: "INPUT", type: "CHECKBOX" }))).toBe(false);
  });

  it("tagName의_대소문자를_가리지_않는다", () => {
    expect(isTextEntryTarget(target({ tagName: "textarea" }))).toBe(true);
  });

  it("보통_요소와_null은_거짓이다", () => {
    expect(isTextEntryTarget(target())).toBe(false);
    expect(isTextEntryTarget(target({ tagName: "BUTTON", role: "button" }))).toBe(false);
    expect(isTextEntryTarget(target({ isContentEditable: false, role: null, type: null }))).toBe(
      false,
    );
    expect(isTextEntryTarget(null)).toBe(false);
  });
});

describe("shortcutsSuppressed", () => {
  const idle = { target: target(), dialogOpen: false, composing: false };

  it("아무_것도_걸리지_않으면_거짓이다", () => {
    expect(shortcutsSuppressed(idle)).toBe(false);
    expect(shortcutsSuppressed({ ...idle, target: null })).toBe(false);
  });

  it("dialog가_열려_있으면_참이다", () => {
    expect(shortcutsSuppressed({ ...idle, dialogOpen: true })).toBe(true);
    expect(shortcutsSuppressed({ ...idle, target: null, dialogOpen: true })).toBe(true);
  });

  it("IME_조합_중이면_참이다", () => {
    expect(shortcutsSuppressed({ ...idle, composing: true })).toBe(true);
  });

  it("dialog_안의_요소면_참이다", () => {
    expect(
      shortcutsSuppressed({ ...idle, target: target({ inDialog: true }) }),
    ).toBe(true);
  });

  it("텍스트_입력이면_참이다", () => {
    // Login, search, and folder-picker fields all use this guard.
    expect(
      shortcutsSuppressed({
        ...idle,
        target: target({ tagName: "INPUT", type: "password" }),
      }),
    ).toBe(true);
  });

  it("체크박스는_막지_않는다", () => {
    expect(
      shortcutsSuppressed({
        ...idle,
        target: target({ tagName: "INPUT", type: "checkbox" }),
      }),
    ).toBe(false);
  });
});
