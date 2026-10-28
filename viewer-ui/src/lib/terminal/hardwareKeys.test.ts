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
  it("Ctrl+Enter sends LF", () => {
    // xterm sends CR, which the TUI interprets as submit.
    expect(overriddenKeySequence(key({ ctrlKey: true }))).toBe("\n");
  });

  it("Ctrl+Alt+Enter prefixes ESC", () => {
    expect(overriddenKeySequence(key({ ctrlKey: true, altKey: true }))).toBe(
      "\x1b\n",
    );
  });

  it("plain Enter is left to xterm", () => {
    expect(overriddenKeySequence(key())).toBeNull();
    expect(overriddenKeySequence(key({ altKey: true }))).toBeNull();
  });

  it("Enter with Meta pressed is left to xterm", () => {
    expect(
      overriddenKeySequence(key({ ctrlKey: true, metaKey: true })),
    ).toBeNull();
  });

  it("keys other than Enter are left to xterm", () => {
    expect(overriddenKeySequence(key({ key: "c", ctrlKey: true }))).toBeNull();
  });

  it("does not send the keypress for the same keystroke twice", () => {
    expect(
      overriddenKeySequence(key({ type: "keypress", ctrlKey: true })),
    ).toBeNull();
  });
});

const LINUX = "Linux x86_64";
const on = (platform: string, event: TypedKey) => browserHandlesKey(event, platform);

describe("browserHandlesKey", () => {
  it("Ctrl+V is left to the browser", () => {
    // Otherwise xterm encodes it as \x16 and no paste event is emitted.
    expect(on(LINUX, key({ key: "v", ctrlKey: true }))).toBe(true);
    expect(on(LINUX, key({ key: "V", ctrlKey: true }))).toBe(true);
  });

  it("v without Ctrl is ordinary typing", () => {
    expect(on(LINUX, key({ key: "v" }))).toBe(false);
  });

  it("a combination with Alt or Meta belongs to the pane", () => {
    expect(
      on(LINUX, key({ key: "v", ctrlKey: true, altKey: true })),
    ).toBe(false);
    expect(
      on(LINUX, key({ key: "v", ctrlKey: true, metaKey: true })),
    ).toBe(false);
  });

  it("does not respond to an event other than keydown", () => {
    expect(
      on(LINUX, key({ type: "keypress", key: "v", ctrlKey: true })),
    ).toBe(false);
  });
});

describe("browserHandlesKey Apple", () => {
  it("on Mac, sends Ctrl+V to the pane", () => {
    // Cmd+V is browser paste, while Ctrl+V tells Claude Code to read the clipboard.
    // Passing it to the browser would send it nowhere.
    for (const platform of ["MacIntel", "iPhone", "iPad"]) {
      expect(on(platform, key({ key: "v", ctrlKey: true })), platform).toBe(false);
    }
  });

  it("on Windows, still leaves it to the browser", () => {
    expect(on("Win32", key({ key: "v", ctrlKey: true }))).toBe(true);
  });
});
