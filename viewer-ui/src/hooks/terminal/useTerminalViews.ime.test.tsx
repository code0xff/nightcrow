// @vitest-environment happy-dom

import { act, cleanup, renderHook } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { Terminal } from "@xterm/xterm";
import type { PaneView } from "../../lib/terminal/terminalLayout";
import {
  createTerminalReplayState,
  type PendingTerminalWrite,
} from "../../lib/terminal/terminalReplay";
import { useTerminalViews } from "./useTerminalViews";

const mounted: { term: Terminal; body: HTMLDivElement }[] = [];

afterEach(() => {
  cleanup();
  mounted.splice(0).forEach(({ term, body }) => {
    term.dispose();
    body.remove();
  });
  vi.unstubAllGlobals();
});

function mountTerminal() {
  const pane = 7;
  const body = document.createElement("div");
  document.body.append(body);
  const viewsRef = { current: new Map<number, PaneView>() };
  const pendingRef = { current: new Map<number, PendingTerminalWrite[]>() };
  const replayRef = { current: createTerminalReplayState() };
  const ptySizesRef = { current: new Map() };
  const sent = vi.fn();
  const socket = {
    readyState: WebSocket.OPEN,
    send: sent,
  } as unknown as WebSocket;
  const args: Parameters<typeof useTerminalViews>[0] = {
    panes: [pane],
    size: { w: 600, h: 300 },
    zoomed: null,
    mode: "grid",
    screenScale: 100,
    socketRef: { current: socket },
    viewsRef,
    bodyRefs: { current: new Map([[pane, body]]) },
    pendingRef,
    replayRef,
    ptySizesRef,
    consumeLatches: (typed) => typed,
    setTitles: () => {},
  };
  renderHook(() => useTerminalViews(args));
  const term = viewsRef.current.get(pane)!.term;
  mounted.push({ term, body });
  return {
    term,
    sentInputs: () =>
      sent.mock.calls
        .map(([frame]) => JSON.parse(String(frame)) as { type: string; data?: string })
        .filter((message) => message.type === "input")
        .map((message) => message.data ?? ""),
  };
}

function compositionEvent(type: "compositionstart" | "compositionupdate" | "compositionend", data: string) {
  return new CompositionEvent(type, { data, bubbles: true });
}

function insertText(
  textarea: HTMLTextAreaElement,
  data: string,
  isComposing = false,
) {
  textarea.dispatchEvent(
    new InputEvent("input", {
      data,
      inputType: "insertText",
      isComposing,
      bubbles: true,
      composed: true,
    }),
  );
}

function keyEvent(type: "keydown" | "keyup", key: string, keyCode: number, ctrlKey = false) {
  const event = new KeyboardEvent(type, { key, ctrlKey, bubbles: true });
  Object.defineProperty(event, "keyCode", { value: keyCode });
  return event;
}

function settle(): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, 10));
}

describe("useTerminalViews IME input", () => {
  it("reproduces xterm's duplicate before guarding its composition input", async () => {
    const term = new Terminal();
    const body = document.createElement("div");
    document.body.append(body);
    term.open(body);
    mounted.push({ term, body });
    const textarea = term.textarea!;
    const data: string[] = [];
    term.onData((value) => data.push(value));
    textarea.dispatchEvent(compositionEvent("compositionstart", ""));
    textarea.value = "안녕";
    textarea.dispatchEvent(compositionEvent("compositionupdate", "안녕"));
    textarea.dispatchEvent(compositionEvent("compositionend", "안녕"));
    insertText(textarea, "안녕");
    await settle();
    expect(data.join("")).toBe("안녕안녕");
  });

  it("guards insertText on either side of compositionend and leaves composition text alone", async () => {
    const { term, sentInputs } = mountTerminal();
    const textarea = term.textarea!;
    textarea.dispatchEvent(compositionEvent("compositionstart", ""));
    textarea.value = "한";
    textarea.dispatchEvent(compositionEvent("compositionupdate", "한"));
    insertText(textarea, "한", true);
    textarea.dispatchEvent(compositionEvent("compositionend", "한"));
    insertText(textarea, "한");
    await act(settle);
    expect(sentInputs()).toEqual(["한"]);

    textarea.value = "";
    textarea.dispatchEvent(compositionEvent("compositionstart", ""));
    textarea.value = "글";
    textarea.dispatchEvent(compositionEvent("compositionupdate", "글"));
    const compositionTextSeen = vi.fn();
    textarea.addEventListener("input", (event) => {
      if ((event as InputEvent).inputType === "insertCompositionText") {
        compositionTextSeen();
      }
    });
    textarea.dispatchEvent(
      new InputEvent("input", {
        data: "글",
        inputType: "insertCompositionText",
        isComposing: true,
        bubbles: true,
        composed: true,
      }),
    );
    textarea.dispatchEvent(compositionEvent("compositionend", "글"));
    await act(settle);
    expect(compositionTextSeen).toHaveBeenCalledOnce();
    expect(sentInputs()).toEqual(["한", "글"]);
  });

  it("suppresses composition insertText but preserves plain repeats and paste events", async () => {
    const { term, sentInputs } = mountTerminal();
    const textarea = term.textarea!;
    textarea.dispatchEvent(compositionEvent("compositionstart", ""));
    textarea.value = "한글";
    textarea.dispatchEvent(compositionEvent("compositionupdate", "한글"));
    textarea.dispatchEvent(compositionEvent("compositionend", "한글"));
    insertText(textarea, "한글");
    await act(settle);
    expect(sentInputs()).toEqual(["한글"]);

    insertText(textarea, "a");
    insertText(textarea, "a");
    const pasteSeen = vi.fn();
    textarea.addEventListener("input", (event) => {
      if ((event as InputEvent).inputType === "insertFromPaste") pasteSeen();
    });
    textarea.dispatchEvent(
      new InputEvent("input", {
        data: "paste",
        inputType: "insertFromPaste",
        bubbles: true,
        composed: true,
      }),
    );
    expect(sentInputs()).toEqual(["한글", "a", "a"]);
    expect(pasteSeen).toHaveBeenCalledOnce();
  });

  it("retains rapid compositions and a same-turn trailing character", async () => {
    const { term, sentInputs } = mountTerminal();
    const textarea = term.textarea!;
    textarea.dispatchEvent(compositionEvent("compositionstart", ""));
    textarea.value = "안녕";
    textarea.dispatchEvent(compositionEvent("compositionupdate", "안녕"));
    textarea.dispatchEvent(compositionEvent("compositionend", "안녕"));
    insertText(textarea, "안녕");
    textarea.dispatchEvent(compositionEvent("compositionstart", ""));
    textarea.value = "안녕하";
    textarea.dispatchEvent(compositionEvent("compositionupdate", "하"));
    textarea.dispatchEvent(compositionEvent("compositionend", "하"));
    insertText(textarea, "하");
    textarea.value += "!";
    insertText(textarea, "!");
    await act(settle);
    expect(sentInputs().join("")).toBe("안녕하!");
  });

  it("releases the guard when xterm immediately finalizes composition on Enter", async () => {
    const { term, sentInputs } = mountTerminal();
    const textarea = term.textarea!;
    textarea.dispatchEvent(compositionEvent("compositionstart", ""));
    textarea.value = "안녕";
    textarea.dispatchEvent(compositionEvent("compositionupdate", "안녕"));
    await settle();
    textarea.dispatchEvent(compositionEvent("compositionend", "안녕"));
    textarea.dispatchEvent(keyEvent("keydown", "Enter", 13));
    textarea.dispatchEvent(keyEvent("keyup", "Enter", 13));
    textarea.value += "!";
    insertText(textarea, "!");
    await act(settle);
    expect(sentInputs()).toEqual(["안녕", "\r", "!"]);
  });

  it("removes its input listener when xterm is disposed", () => {
    const { term } = mountTerminal();
    const textarea = term.textarea!;
    textarea.dispatchEvent(compositionEvent("compositionstart", ""));
    const inputSeen = vi.fn();
    textarea.addEventListener("input", inputSeen);
    term.dispose();
    insertText(textarea, "a");
    expect(inputSeen).toHaveBeenCalledOnce();
  });

  it("keeps transaction ownership for xterm's 229 and overridden-key paths", async () => {
    const { term, sentInputs } = mountTerminal();
    const textarea = term.textarea!;
    textarea.dispatchEvent(compositionEvent("compositionstart", ""));
    textarea.value = "한글";
    textarea.dispatchEvent(compositionEvent("compositionupdate", "한글"));
    await settle();
    textarea.dispatchEvent(keyEvent("keydown", "Enter", 229));
    textarea.dispatchEvent(compositionEvent("compositionend", "한글"));
    insertText(textarea, "한글");
    await act(settle);
    expect(sentInputs()).toEqual(["한글"]);

    textarea.dispatchEvent(keyEvent("keyup", "Enter", 229));
    textarea.value = "";
    textarea.dispatchEvent(compositionEvent("compositionstart", ""));
    textarea.value = "안녕";
    textarea.dispatchEvent(compositionEvent("compositionupdate", "안녕"));
    await settle();
    textarea.dispatchEvent(compositionEvent("compositionend", "안녕"));
    textarea.dispatchEvent(keyEvent("keydown", "Enter", 13, true));
    textarea.dispatchEvent(keyEvent("keyup", "Enter", 13, true));
    textarea.value += "!";
    insertText(textarea, "!");
    await act(settle);
    expect(sentInputs().slice(1).join("")).toBe("\n안녕!");
  });
});
