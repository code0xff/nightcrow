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
  it("an element that receives typing returns true", () => {
    expect(isTextEntryTarget(target({ tagName: "INPUT" }))).toBe(true);
    expect(isTextEntryTarget(target({ tagName: "TEXTAREA" }))).toBe(true);
    expect(isTextEntryTarget(target({ tagName: "SELECT" }))).toBe(true);
    expect(isTextEntryTarget(target({ isContentEditable: true }))).toBe(true);
  });

  it("an element with a text-input role returns true", () => {
    for (const role of ["textbox", "searchbox", "combobox"]) {
      expect(isTextEntryTarget(target({ role }))).toBe(true);
    }
    expect(isTextEntryTarget(target({ role: "SearchBox" }))).toBe(true);
  });

  it("a text input with a type returns true", () => {
    for (const type of ["text", "password", "search", "email", "url", "number"]) {
      expect(isTextEntryTarget(target({ tagName: "INPUT", type }))).toBe(true);
    }
  });

  it("an input that does not accept text returns false", () => {
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

  it("tagName matching is case-insensitive", () => {
    expect(isTextEntryTarget(target({ tagName: "textarea" }))).toBe(true);
  });

  it("ordinary elements and null return false", () => {
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

  it("returns false when nothing matches", () => {
    expect(shortcutsSuppressed(idle)).toBe(false);
    expect(shortcutsSuppressed({ ...idle, target: null })).toBe(false);
  });

  it("returns true when a dialog is open", () => {
    expect(shortcutsSuppressed({ ...idle, dialogOpen: true })).toBe(true);
    expect(shortcutsSuppressed({ ...idle, target: null, dialogOpen: true })).toBe(true);
  });

  it("returns true during IME composition", () => {
    expect(shortcutsSuppressed({ ...idle, composing: true })).toBe(true);
  });

  it("returns true for an element inside a dialog", () => {
    expect(
      shortcutsSuppressed({ ...idle, target: target({ inDialog: true }) }),
    ).toBe(true);
  });

  it("returns true for text input", () => {
    // Login, search, and folder-picker fields all use this guard.
    expect(
      shortcutsSuppressed({
        ...idle,
        target: target({ tagName: "INPUT", type: "password" }),
      }),
    ).toBe(true);
  });

  it("does not block checkboxes", () => {
    expect(
      shortcutsSuppressed({
        ...idle,
        target: target({ tagName: "INPUT", type: "checkbox" }),
      }),
    ).toBe(false);
  });
});
