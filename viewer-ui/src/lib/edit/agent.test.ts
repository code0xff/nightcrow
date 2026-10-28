// @vitest-environment happy-dom
//
// The agent runs inside the preview frame; here it runs against this document,
// with `parent` being this window, so what it posts arrives as messages here.

import { afterEach, describe, expect, it } from "vitest";
import { previewAgent } from "./agent";

/** Let posted messages and the agent's own deferred work land. */
const settle = () => new Promise((resolve) => setTimeout(resolve, 0));

let detach: (() => void) | null = null;

afterEach(() => {
  detach?.();
  detach = null;
  document.body.innerHTML = "";
});

/** Mount the agent over one editable block and let verification pass. */
async function openBlock() {
  document.body.innerHTML = '<p data-ne-id="0">Hello</p>';
  const received: Record<string, unknown>[] = [];
  window.addEventListener("message", (event) => {
    received.push(event.data as Record<string, unknown>);
  });
  detach = previewAgent();
  // Dispatched rather than posted: the agent accepts only what `parent` sent,
  // and this environment does not stamp a source on a posted message.
  window.dispatchEvent(
    new MessageEvent("message", {
      data: { type: "locked", ids: [], all: [0] },
      source: window,
    }),
  );
  await settle();
  const block = document.querySelector("p") as HTMLParagraphElement;
  return { block, received };
}

describe("previewAgent commit", () => {
  it("commits once when a block loses editability and blurs", async () => {
    const { block, received } = await openBlock();
    // What Chrome does: a focused element that stops being editable is blurred
    // on the spot, and the focusout reaches the document's handlers before
    // `removeAttribute` returns — that handler is the one that commits.
    const remove = block.removeAttribute.bind(block);
    block.removeAttribute = (name: string) => {
      remove(name);
      if (name === "contenteditable") {
        block.dispatchEvent(new FocusEvent("focusout", { bubbles: true }));
      }
    };

    block.dispatchEvent(new MouseEvent("click", { bubbles: true }));
    expect(block.getAttribute("contenteditable")).toBe("true");
    block.textContent = "Changed";
    block.dispatchEvent(new FocusEvent("focusout", { bubbles: true }));
    await settle();

    const edits = received.filter((m) => m.type === "edit");
    expect(edits).toEqual([{ type: "edit", id: 0, html: "Changed", pristine: false }]);
  });

  it("cancel restores the original content without committing, even on blur", async () => {
    const { block, received } = await openBlock();
    const remove = block.removeAttribute.bind(block);
    block.removeAttribute = (name: string) => {
      remove(name);
      if (name === "contenteditable") {
        block.dispatchEvent(new FocusEvent("focusout", { bubbles: true }));
      }
    };

    block.dispatchEvent(new MouseEvent("click", { bubbles: true }));
    block.textContent = "Changed";
    block.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true }));
    await settle();

    expect(block.textContent).toBe("Hello");
    expect(received.filter((m) => m.type === "edit")).toEqual([]);
  });

  it("sends the IME commit and flush response in the same order after compositionend", async () => {
    const { block, received } = await openBlock();
    block.dispatchEvent(new MouseEvent("click", { bubbles: true }));
    document.dispatchEvent(new CompositionEvent("compositionstart", { bubbles: true }));
    block.textContent = "\uC870\uD569 \uC911";
    window.dispatchEvent(
      new MessageEvent("message", {
        data: { type: "flush", seq: 7 },
        source: window,
      }),
    );
    await settle();
    expect(received.some((message) => message.type === "edit")).toBe(false);
    expect(received.some((message) => message.type === "flushed")).toBe(false);

    document.dispatchEvent(new CompositionEvent("compositionend", { bubbles: true }));
    await settle();
    const completion = received.filter((message) => message.type === "edit" || message.type === "flushed");
    expect(completion).toEqual([
      { type: "edit", id: 0, html: "\uC870\uD569 \uC911", pristine: false },
      { type: "flushed", seq: 7 },
    ]);
  });
});
