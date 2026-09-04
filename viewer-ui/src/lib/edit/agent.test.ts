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
  it("블록이_편집_가능성을_잃으며_blur되어도_커밋은_한_번이다", async () => {
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

  it("취소는_blur가_되어도_커밋하지_않고_원래_내용을_되돌린다", async () => {
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
});
