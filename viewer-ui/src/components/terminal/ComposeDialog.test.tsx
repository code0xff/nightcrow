// @vitest-environment happy-dom

import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { useState } from "react";
import { ComposeDialog } from "./ComposeDialog";

afterEach(cleanup);

function mount(initial = "", sendResult = true) {
  const onSend = vi.fn(() => sendResult);
  const onClose = vi.fn();
  function Host() {
    const [draft, setDraft] = useState(initial);
    return (
      <ComposeDialog
        label="terminal 1"
        draft={draft}
        onChange={setDraft}
        onSend={onSend}
        onClose={onClose}
      />
    );
  }
  // Mounted inside a panel, as it is in the app, so the portal is what is tested.
  render(
    <section data-terminal-panel="">
      <Host />
    </section>,
  );
  const field = screen.getByRole("textbox", { name: "message" }) as HTMLTextAreaElement;
  return { field, onSend, onClose };
}

describe("ComposeDialog", () => {
  it("send_is_disabled_until_there_is_something_to_send", () => {
    const { field } = mount();
    const send = screen.getByRole("button", { name: /send/i }) as HTMLButtonElement;
    expect(send.disabled).toBe(true);
    fireEvent.change(field, { target: { value: "  " } });
    expect(send.disabled).toBe(true);
    fireEvent.change(field, { target: { value: "hi" } });
    expect(send.disabled).toBe(false);
  });

  it("clear_empties_the_field", () => {
    const { field } = mount("some text");
    fireEvent.click(screen.getByRole("button", { name: "Clear" }));
    expect(field.value).toBe("");
  });

  it("return_alone_does_not_send", () => {
    const { field, onSend } = mount("hi");
    fireEvent.keyDown(field, { key: "Enter" });
    expect(onSend).not.toHaveBeenCalled();
  });

  it("cmd_or_ctrl_return_sends", () => {
    const { field, onSend } = mount("hi");
    fireEvent.keyDown(field, { key: "Enter", metaKey: true });
    fireEvent.keyDown(field, { key: "Enter", ctrlKey: true });
    expect(onSend).toHaveBeenCalledTimes(2);
  });

  it("return_that_confirms_an_ime_composition_is_left_to_the_ime", () => {
    const { field, onSend } = mount("한");
    fireEvent.keyDown(field, { key: "Enter", metaKey: true, isComposing: true });
    expect(onSend).not.toHaveBeenCalled();
  });

  it("the_send_chord_is_written_in_the_dialog", () => {
    mount();
    expect(screen.getByText(/to send/)).toBeTruthy();
  });

  it("a_failed_send_says_nothing_went_out_in_place_of_the_chord", () => {
    mount("hi", false);
    fireEvent.click(screen.getByRole("button", { name: /send/i }));
    expect(screen.getByRole("alert").textContent).toMatch(/nothing was sent/);
    expect(screen.queryByText(/to send/)).toBeNull();
  });

  it("escape_closes_the_dialog", () => {
    const { onClose } = mount();
    fireEvent.keyDown(document, { key: "Escape" });
    expect(onClose).toHaveBeenCalled();
  });

  it("the_dialog_is_rendered_outside_the_terminal_panel", () => {
    const { field } = mount();
    expect(field.closest("[data-terminal-panel]")).toBeNull();
    expect(document.activeElement).toBe(field);
  });
});
