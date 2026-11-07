// @vitest-environment happy-dom

import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { useState } from "react";
import { ComposeDialog } from "./ComposeDialog";
import type { DictationAvailability, Recognizer } from "../../lib/terminal/dictation";

afterEach(cleanup);

function fakeRecognizer() {
  const r: Recognizer = {
    lang: "",
    continuous: true,
    interimResults: true,
    onresult: null,
    onerror: null,
    onend: null,
    start: vi.fn(),
    stop: vi.fn(() => r.onend?.()),
    abort: vi.fn(() => r.onend?.()),
  };
  return r;
}

function mount(availability: DictationAvailability, initial = "", startThrows = false) {
  const recognizers: Recognizer[] = [];
  const factory = () => {
    const r = fakeRecognizer();
    if (startThrows) r.start = vi.fn(() => { throw new Error("busy"); });
    recognizers.push(r);
    return r;
  };
  function Host() {
    const [draft, setDraft] = useState(initial);
    return (
      <ComposeDialog
        label="terminal 1"
        draft={draft}
        onChange={setDraft}
        onSend={() => true}
        onAttachImage={() => {}}
        uploading={false}
        uploadError={null}
        onDismissUploadError={() => {}}
        onClose={() => {}}
        dictation={{ availability, factory }}
      />
    );
  }
  const view = render(<Host />);
  const field = screen.getByRole("textbox", { name: "message" }) as HTMLTextAreaElement;
  return { field, recognizers, view };
}

function say(r: Recognizer, text: string) {
  act(() => r.onresult?.({ resultIndex: 0, results: [{ isFinal: true, 0: { transcript: text } }] }));
}

describe("DictationButton", () => {
  it("is_absent_where_the_browser_cannot_recognize_speech", () => {
    mount("unsupported");
    expect(screen.queryByRole("button", { name: /voice input/i })).toBeNull();
  });

  it("explains_https_instead_of_listening_on_plain_http", () => {
    const { recognizers } = mount("insecure");
    const button = screen.getByRole("button", { name: "Voice input needs HTTPS" });
    expect(button.getAttribute("aria-disabled")).toBeNull();
    fireEvent.click(button);
    expect(recognizers).toHaveLength(0);
    expect(screen.getByRole("alert").textContent).toMatch(/HTTPS or localhost/);
  });

  it("appends_final_text_to_the_draft_without_sending", () => {
    const { field, recognizers } = mount("ready", "git");
    fireEvent.click(screen.getByRole("button", { name: "Voice input" }));
    const r = recognizers[0];
    expect(r.start).toHaveBeenCalled();
    expect(r.continuous).toBe(false);
    expect(r.interimResults).toBe(false);
    expect(screen.getByRole("button", { name: "Stop voice input" })).toBeTruthy();
    say(r, "status");
    act(() => r.onend?.());
    expect(field.value).toBe("git status");
    expect(screen.getByRole("button", { name: "Voice input" })).toBeTruthy();
  });

  it("pressing_again_stops_listening", () => {
    const { recognizers } = mount("ready");
    fireEvent.click(screen.getByRole("button", { name: "Voice input" }));
    fireEvent.click(screen.getByRole("button", { name: "Stop voice input" }));
    // Stopped, not aborted, so a phrase still being recognized is kept.
    expect(recognizers[0].stop).toHaveBeenCalled();
    expect(recognizers[0].abort).not.toHaveBeenCalled();
    expect(screen.getByRole("button", { name: "Voice input" })).toBeTruthy();
  });

  it("reports_a_blocked_microphone", () => {
    const { recognizers } = mount("ready");
    fireEvent.click(screen.getByRole("button", { name: "Voice input" }));
    act(() => recognizers[0].onerror?.({ error: "not-allowed" }));
    act(() => recognizers[0].onend?.());
    expect(screen.getByRole("alert").textContent).toMatch(/blocked/);
  });

  it("reports_a_start_that_throws_and_does_not_claim_to_listen", () => {
    mount("ready", "", true);
    fireEvent.click(screen.getByRole("button", { name: "Voice input" }));
    expect(screen.getByRole("alert").textContent).toMatch(/failed/);
    expect(screen.getByRole("button", { name: "Voice input" })).toBeTruthy();
  });

  it("closing_the_form_releases_the_microphone", () => {
    const { recognizers, view } = mount("ready");
    fireEvent.click(screen.getByRole("button", { name: "Voice input" }));
    view.unmount();
    expect(recognizers[0].abort).toHaveBeenCalled();
  });
});
