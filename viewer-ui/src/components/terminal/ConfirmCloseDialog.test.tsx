// @vitest-environment happy-dom

import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { ConfirmCloseDialog } from "./ConfirmCloseDialog";

afterEach(cleanup);

function mount() {
  const onConfirm = vi.fn();
  const onCancel = vi.fn();
  render(
    <section data-terminal-panel="">
      <ConfirmCloseDialog label="terminal 1" onConfirm={onConfirm} onCancel={onCancel} />
    </section>,
  );
  return { onConfirm, onCancel };
}

describe("ConfirmCloseDialog", () => {
  it("names_the_pane_and_focuses_close", () => {
    mount();
    expect(screen.getByRole("alertdialog", { name: "Close terminal 1?" })).toBeTruthy();
    expect(document.activeElement).toBe(screen.getByRole("button", { name: "Close" }));
  });

  it("close_confirms", () => {
    const { onConfirm, onCancel } = mount();
    fireEvent.click(screen.getByRole("button", { name: "Close" }));
    expect(onConfirm).toHaveBeenCalledOnce();
    expect(onCancel).not.toHaveBeenCalled();
  });

  it("cancel_escape_and_backdrop_do_not_close", () => {
    const { onConfirm, onCancel } = mount();
    fireEvent.click(screen.getByRole("button", { name: "Cancel" }));
    fireEvent.keyDown(document, { key: "Escape" });
    fireEvent.click(screen.getByRole("alertdialog").parentElement!);
    expect(onCancel).toHaveBeenCalledTimes(3);
    expect(onConfirm).not.toHaveBeenCalled();
  });
});
