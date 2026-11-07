import { describe, expect, it } from "vitest";
import {
  appendDictation,
  dictationAvailability,
  dictationErrorMessage,
  finalTranscript,
  type Recognizer,
} from "./dictation";

const factory = () => ({}) as Recognizer;

function event(resultIndex: number, results: [string, boolean][]) {
  return {
    resultIndex,
    results: results.map(([transcript, isFinal]) => ({ isFinal, 0: { transcript } })),
  };
}

describe("dictationAvailability", () => {
  it("http_is_named_only_where_https_would_help", () => {
    expect(dictationAvailability(false, factory)).toBe("insecure");
    expect(dictationAvailability(false, null)).toBe("unsupported");
  });

  it("a_secure_page_depends_on_the_browser", () => {
    expect(dictationAvailability(true, factory)).toBe("ready");
    expect(dictationAvailability(true, null)).toBe("unsupported");
  });
});

describe("finalTranscript", () => {
  it("keeps_only_final_results_from_the_changed_index", () => {
    expect(finalTranscript(event(1, [["old", true], ["안녕 ", true], ["하세", false]]))).toBe("안녕");
  });

  it("is_empty_without_a_final_result", () => {
    expect(finalTranscript(event(0, [["partial", false]]))).toBe("");
  });
});

describe("appendDictation", () => {
  it("adds_a_space_only_where_one_is_missing", () => {
    expect(appendDictation("", "hi")).toBe("hi");
    expect(appendDictation("run", "tests")).toBe("run tests");
    expect(appendDictation("run ", "tests")).toBe("run tests");
    expect(appendDictation("line\n", "next")).toBe("line\nnext");
  });

  it("leaves_the_draft_alone_for_empty_text", () => {
    expect(appendDictation("draft", "")).toBe("draft");
  });
});

describe("dictationErrorMessage", () => {
  it("is_silent_for_our_own_stop_and_for_silence", () => {
    expect(dictationErrorMessage("aborted")).toBeNull();
    expect(dictationErrorMessage("no-speech")).toBeNull();
  });

  it("explains_a_blocked_microphone_and_unknown_codes", () => {
    expect(dictationErrorMessage("not-allowed")).toMatch(/blocked/);
    expect(dictationErrorMessage("weird")).toMatch(/weird/);
  });
});
