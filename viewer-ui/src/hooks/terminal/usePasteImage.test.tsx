// @vitest-environment happy-dom

import { renderHook } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { usePasteImage } from "./usePasteImage";
import type { PaneView } from "../../lib/terminalLayout";

const { pasteImage, toastError } = vi.hoisted(() => ({
  pasteImage: vi.fn(),
  toastError: vi.fn(),
}));
vi.mock("../../api", () => ({ api: { pasteImage } }));
vi.mock("../../lib/toast", () => ({ toast: { error: toastError } }));

function fakeSocket() {
  const sent: unknown[] = [];
  const socket = {
    readyState: WebSocket.OPEN,
    send: (raw: string) => sent.push(JSON.parse(raw)),
  } as unknown as WebSocket;
  return { socket, sent };
}

/** A panel holding one pane cell, the shape `paneOf` reads. */
function panel(): { container: HTMLDivElement; inPane: HTMLElement } {
  const container = document.createElement("div");
  const cell = document.createElement("div");
  cell.setAttribute("data-pane-id", "7");
  const inPane = document.createElement("textarea");
  cell.appendChild(inPane);
  container.appendChild(cell);
  document.body.appendChild(container);
  return { container, inPane };
}

function clipboard(files: File[], text = "") {
  return {
    files,
    getData: (kind: string) => (kind === "text/plain" ? text : ""),
  };
}

function pasteOn(target: HTMLElement, data: ReturnType<typeof clipboard>) {
  const event = new Event("paste", { bubbles: true, cancelable: true });
  Object.defineProperty(event, "clipboardData", { value: data });
  target.dispatchEvent(event);
  return event;
}

const png = () => new File([new Uint8Array([1])], "shot.png", { type: "image/png" });

function mount(container: HTMLDivElement) {
  const { socket, sent } = fakeSocket();
  const views = new Map<number, PaneView>([
    [7, { term: { modes: { bracketedPasteMode: true } } } as unknown as PaneView],
  ]);
  renderHook(() =>
    usePasteImage({
      containerRef: { current: container },
      socketRef: { current: socket },
      viewsRef: { current: views },
    }),
  );
  return sent;
}

beforeEach(() => {
  pasteImage.mockReset();
  toastError.mockReset();
});
afterEach(() => document.body.replaceChildren());

describe("usePasteImage", () => {
  it("an_image_pasted_in_a_pane_is_uploaded_and_its_path_typed_into_that_pane", async () => {
    pasteImage.mockResolvedValue("/home/me/.nightcrow/tmp/paste-ab.png");
    const { container, inPane } = panel();
    const sent = mount(container);

    const event = pasteOn(inPane, clipboard([png()]));
    await vi.waitFor(() => expect(sent).toHaveLength(1));

    expect(event.defaultPrevented).toBe(true);
    expect(sent[0]).toEqual({
      type: "input",
      pane: 7,
      // Bracketed, because the pane's program asked for mode 2004.
      data: "\x1b[200~/home/me/.nightcrow/tmp/paste-ab.png \x1b[201~",
    });
  });

  it("a_text_paste_is_left_to_the_terminal", () => {
    const { container, inPane } = panel();
    const sent = mount(container);

    const event = pasteOn(inPane, clipboard([], "some words"));

    expect(event.defaultPrevented).toBe(false);
    expect(pasteImage).not.toHaveBeenCalled();
    expect(sent).toHaveLength(0);
  });

  it("an_image_pasted_outside_any_pane_is_not_claimed", () => {
    const { container } = panel();
    const outside = document.createElement("div");
    container.appendChild(outside);
    const sent = mount(container);

    const event = pasteOn(outside, clipboard([png()]));

    expect(event.defaultPrevented).toBe(false);
    expect(pasteImage).not.toHaveBeenCalled();
    expect(sent).toHaveLength(0);
  });

  it("a_failed_upload_types_nothing_into_the_pane", async () => {
    pasteImage.mockImplementation(async () => {
      throw new Error("too large");
    });
    const { container, inPane } = panel();
    const sent = mount(container);

    pasteOn(inPane, clipboard([png()]));
    await vi.waitFor(() => expect(pasteImage).toHaveBeenCalled());

    expect(sent).toHaveLength(0);
    expect(toastError).toHaveBeenCalledWith(
      expect.stringContaining("too large"),
    );
  });
});
