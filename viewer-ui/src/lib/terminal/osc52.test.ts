import { describe, expect, it } from "vitest";
import { parseOsc52 } from "./osc52";

describe("parseOsc52", () => {
  it("decodes a clipboard-targeting payload as text", () => {
    expect(parseOsc52("c;Z2l0IHN0YXR1cw==")).toEqual({
      kind: "write",
      text: "git status",
    });
  });

  it("decodes Korean text as characters rather than bytes", () => {
    // Base64 carries bytes, so decoding without UTF-8 would corrupt the text.
    expect(parseOsc52("c;7JWI64WV7ZWY7IS47JqU")).toEqual({
      kind: "write",
      text: "\uC548\uB155\uD558\uC138\uC694",
    });
  });

  it("uses the clipboard when the selector is empty", () => {
    // The spec defines an omitted selector as `s0`, but this page has only one clipboard,
    // so this intentionally maps it to the target the program intended.
    expect(parseOsc52(";aGVsbG8=")).toEqual({ kind: "write", text: "hello" });
  });

  it("the select buffer also maps to the same clipboard", () => {
    expect(parseOsc52("s;aGVsbG8=")).toEqual({ kind: "write", text: "hello" });
  });

  it("does not answer read queries", () => {
    // Answering would expose the last copied value to the program in the pane.
    expect(parseOsc52("c;?")).toEqual({ kind: "ignore" });
  });

  it("does not map an unsupported browser selector to the clipboard", () => {
    // primary is X11’s middle-click buffer. A program asking for it is not asking for this page’s
    // clipboard.
    expect(parseOsc52("p;aGVsbG8=")).toEqual({ kind: "ignore" });
    expect(parseOsc52("q;aGVsbG8=")).toEqual({ kind: "ignore" });
  });

  it("accepts a payload when one of several selectors targets the clipboard", () => {
    expect(parseOsc52("pc;aGVsbG8=")).toEqual({ kind: "write", text: "hello" });
  });

  it("a selector containing an undefined character is invalid", () => {
    // does not overwrite another selection based only on a `c` in the middle.
    expect(parseOsc52("cX;aGVsbG8=")).toEqual({ kind: "ignore" });
    expect(parseOsc52("notc;aGVsbG8=")).toEqual({ kind: "ignore" });
  });

  it("does not clear the clipboard with empty data", () => {
    expect(parseOsc52("c;")).toEqual({ kind: "ignore" });
  });

  it("ignores a payload with no separator", () => {
    expect(parseOsc52("aGVsbG8=")).toEqual({ kind: "ignore" });
    expect(parseOsc52("")).toEqual({ kind: "ignore" });
  });

  it("ignores non-Base64 data", () => {
    expect(parseOsc52("c;not base64!!")).toEqual({ kind: "ignore" });
  });

  it("ignores bytes that are not valid UTF-8", () => {
    // A clipboard filled with replacement characters is worse than leaving it untouched.
    expect(parseOsc52("c;//4=")).toEqual({ kind: "ignore" });
  });
});
