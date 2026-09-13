import { describe, expect, it } from "vitest";
import { composedInput, isSendable } from "./composeInput";

describe("composedInput", () => {
  it("bracketed_mode_wraps_the_message_in_paste_markers", () => {
    expect(composedInput("안녕하세요", true)).toBe("\x1b[200~안녕하세요\x1b[201~");
  });

  it("unbracketed_mode_sends_the_text_as_is", () => {
    expect(composedInput("hello", false)).toBe("hello");
  });

  it("line_breaks_become_carriage_returns_whatever_their_form", () => {
    expect(composedInput("a\nb\r\nc", true)).toBe("\x1b[200~a\rb\rc\x1b[201~");
  });

  it("an_embedded_paste_end_cannot_close_the_paste_early", () => {
    const sent = composedInput("x\x1b[201~rm -rf .", true);
    expect(sent).toBe("\x1b[200~x[201~rm -rf .\x1b[201~");
    expect(sent.indexOf("\x1b[201~")).toBe(sent.length - "\x1b[201~".length);
  });

  it("an_empty_message_is_still_well_formed", () => {
    expect(composedInput("", true)).toBe("\x1b[200~\x1b[201~");
  });
});

describe("isSendable", () => {
  it("blank_or_whitespace_only_text_is_not_sendable", () => {
    expect(isSendable("")).toBe(false);
    expect(isSendable("  \n\t ")).toBe(false);
  });

  it("any_visible_character_is_sendable", () => {
    expect(isSendable(" 가 ")).toBe(true);
  });
});
